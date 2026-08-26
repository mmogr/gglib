//! Resident-set behaviour that does not need a llama-server.
//!
//! The launch sequence itself is untestable without spawning a process, so what
//! is exercised here is everything around it: the pin guard, the context
//! resolution every admission depends on, and the memory arithmetic that
//! decides whether a second model may co-reside. The scheduling rules live next
//! door in `admission`, and are tested there.

use super::*;
use async_trait::async_trait;
use gglib_core::ports::{CatalogError, ModelSummary};

#[derive(Debug)]
struct StubCatalog;

#[async_trait]
impl ModelCatalogPort for StubCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(Vec::new())
    }
    async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(None)
    }
    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

fn swapping_set() -> ResidentSet {
    ResidentSet::new(
        Arc::new(StubCatalog),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    )
}

fn pinned_set(model: &str) -> ResidentSet {
    let set = swapping_set();
    set.set_pin(Some(PinnedSpec {
        name: model.to_string(),
        launch_overrides: ServerConfigOptions::default(),
    }));
    set
}

// ── the pin ───────────────────────────────────────────────────────────────

#[test]
fn pinned_state_admits_its_own_model() {
    assert!(pinned_set("qwen2.5").check_pinned("qwen2.5").is_ok());
}

#[test]
fn pinned_state_rejects_a_foreign_model() {
    let err = pinned_set("qwen2.5")
        .check_pinned("llama-3-8b")
        .expect_err("a foreign model must be refused");

    match err {
        ModelRuntimeError::PinnedModelMismatch {
            expected,
            requested,
        } => {
            assert_eq!(expected, "qwen2.5");
            assert_eq!(requested, "llama-3-8b");
        }
        other => panic!("expected PinnedModelMismatch, got {other:?}"),
    }
}

/// Matching is exact: a pinned endpoint must not quietly accept a near-miss and
/// serve a different model than the caller named.
#[test]
fn pinned_matching_is_exact() {
    let set = pinned_set("qwen2.5");
    assert!(set.check_pinned("Qwen2.5").is_err(), "case differs");
    assert!(set.check_pinned("qwen2.5-coder").is_err(), "suffix added");
    assert!(set.check_pinned("qwen2").is_err(), "prefix only");
}

/// The unpinned proxy must keep admitting freely — pinning is opt-in.
#[test]
fn unpinned_state_admits_any_model() {
    let set = swapping_set();
    assert!(set.check_pinned("anything").is_ok());
    assert!(set.check_pinned("something-else").is_ok());
}

/// Pinning changes only the admission check; the standing template a pinned
/// server starts from must be identical to the unpinned one.
#[test]
fn pinning_does_not_alter_launch_configuration() {
    let template = ServerConfigOptions {
        mlock: Some(true),
        cache_reuse: Some(256),
        ..Default::default()
    };
    let set = ResidentSet::new(
        Arc::new(StubCatalog),
        template.clone(),
        CacheRamSetting::ExplicitMb(4096),
    );
    set.set_pin(Some(PinnedSpec {
        name: "qwen2.5".to_string(),
        launch_overrides: ServerConfigOptions::default(),
    }));

    assert_eq!(set.launch_overrides.mlock, template.mlock);
    assert_eq!(set.launch_overrides.cache_reuse, template.cache_reuse);
    assert_eq!(set.cache_ram, CacheRamSetting::ExplicitMb(4096));
}

/// Clearing the pin restores ordinary auto-swapping admission.
#[test]
fn clearing_the_pin_restores_auto_swapping() {
    let set = pinned_set("qwen2.5");
    assert!(set.check_pinned("llama-3-8b").is_err());

    set.set_pin(None);

    assert!(set.check_pinned("llama-3-8b").is_ok());
    assert_eq!(set.pinned_name(), None);
}

/// Re-pinning replaces the previous pin rather than accumulating.
#[test]
fn repinning_replaces_the_previous_pin() {
    let set = pinned_set("qwen2.5");
    set.set_pin(Some(PinnedSpec {
        name: "llama-3-8b".to_string(),
        launch_overrides: ServerConfigOptions::default(),
    }));

    assert!(set.check_pinned("llama-3-8b").is_ok());
    assert!(set.check_pinned("qwen2.5").is_err());
}

// ── a fresh set ───────────────────────────────────────────────────────────

#[test]
fn a_fresh_set_is_empty() {
    let set = swapping_set();
    assert!(set.current_model().is_none());
    assert!(!set.queue().is_loading());
    assert!(set.queue().snapshot().slots.is_empty());
}

// ── resolve_launch_opts (#685: the context-bookkeeping desync) ─────────────

/// The actual regression: with no per-request `num_ctx` override, a per-model
/// `server_defaults.context_length` must still win over the global default —
/// this is what `effective_ctx` used to skip entirely
/// (`num_ctx.unwrap_or(default_ctx)` never consulted it), so the tracked
/// context silently disagreed with what `build_server_config` actually launched
/// llama-server with.
#[test]
fn resolve_launch_opts_applies_the_per_model_context_when_nothing_more_specific_is_set() {
    let (opts, resolved_ctx, _) = resolve_launch_opts(
        &ServerConfigOptions::default(),
        &ServerConfigOptions::default(),
        None,          // no per-request override
        Some(131_072), // global default
        None,          // nothing fitted
        Some(196_608), // per-model server_defaults.context_length
    );

    assert_eq!(resolved_ctx, 196_608, "per-model tier must win over global");
    // `opts` is exactly what `build_server_config` resolves from — same fields
    // must still be set on it, or the two callers could diverge.
    assert_eq!(opts.model_server_ctx, Some(196_608));
    assert_eq!(opts.global_default_ctx, Some(131_072));
}

/// A per-request `num_ctx` still outranks the per-model tier — the 4-level
/// chain (`resolve_context_size`) is unchanged by M9, only who reads the result.
#[test]
fn resolve_launch_opts_explicit_num_ctx_beats_the_per_model_tier() {
    let (_, resolved_ctx, _) = resolve_launch_opts(
        &ServerConfigOptions::default(),
        &ServerConfigOptions::default(),
        Some(8_192),
        Some(131_072),
        None,
        Some(196_608),
    );

    assert_eq!(resolved_ctx, 8_192);
}

/// With nothing else set, the global default is what's left.
#[test]
fn resolve_launch_opts_falls_back_to_the_global_default() {
    let (_, resolved_ctx, _) = resolve_launch_opts(
        &ServerConfigOptions::default(),
        &ServerConfigOptions::default(),
        None,
        Some(131_072),
        None,
        None,
    );

    assert_eq!(resolved_ctx, 131_072);
}

/// A stale `model_server_ctx` left over on `template` from a previous model
/// must not leak into this launch — `model_server_ctx` is always assigned from
/// this call's own parameter, never overlaid. With two models resident this
/// matters more than it did: the template is shared between them.
#[test]
fn resolve_launch_opts_never_inherits_a_stale_model_server_ctx_from_the_template() {
    let stale_template = ServerConfigOptions {
        model_server_ctx: Some(4_096), // some other model's context
        ..Default::default()
    };

    let (opts, resolved_ctx, _) = resolve_launch_opts(
        &stale_template,
        &ServerConfigOptions::default(),
        None,
        Some(131_072),
        None,
        None,
    );

    assert_eq!(
        opts.model_server_ctx, None,
        "must not inherit the stale value"
    );
    assert_eq!(resolved_ctx, 131_072);
}

/// The rung this whole change exists to make reachable.
///
/// Before, every caller pre-resolved the global default, so `resolve_launch_opts`
/// received `Some(4096)` whether or not anyone had chosen it — and the fitted
/// value, like the built-in floor below it, was unreachable dead code.
#[test]
fn resolve_launch_opts_carries_the_fitted_value_when_no_user_setting_exists() {
    let (opts, resolved_ctx, source) = resolve_launch_opts(
        &ServerConfigOptions::default(),
        &ServerConfigOptions::default(),
        None,         // no per-request override
        None,         // the user set no global default
        Some(32_768), // fitted to this machine
        None,         // no per-model server_defaults
    );

    assert_eq!(resolved_ctx, 32_768);
    assert_eq!(source, ContextSizeSource::FittedToHardware);
    assert_eq!(opts.fitted_ctx, Some(32_768));
}

/// A number the user typed outranks one gglib computed.
#[test]
fn resolve_launch_opts_never_lets_the_fitted_value_override_a_user_set_global_default() {
    let (_, resolved_ctx, source) = resolve_launch_opts(
        &ServerConfigOptions::default(),
        &ServerConfigOptions::default(),
        None,
        Some(8192),   // the user chose this
        Some(65_536), // and this machine could do far more
        None,
    );

    assert_eq!(resolved_ctx, 8192);
    assert_eq!(source, ContextSizeSource::GlobalDefault);
}

/// A stale fitted value inherited from the standing template would size a
/// launch for a different model than the one being launched.
#[test]
fn resolve_launch_opts_never_inherits_a_stale_fitted_ctx_from_the_template() {
    let template = ServerConfigOptions {
        fitted_ctx: Some(131_072), // left over from some other model
        ..Default::default()
    };
    let (opts, resolved_ctx, _) = resolve_launch_opts(
        &template,
        &ServerConfigOptions::default(),
        None,
        None,
        Some(16_384), // what *this* model fits
        None,
    );

    assert_eq!(opts.fitted_ctx, Some(16_384));
    assert_eq!(resolved_ctx, 16_384);
}

/// The fallback: when the co-resident reservation leaves too little to fit at
/// all, the model is sized against the undivided device rather than dropping
/// to the built-in floor.
///
/// That machine is precisely the one where a full-ceiling secondary could
/// never have loaded, so the reservation buys nothing and costs everything.
#[test]
fn a_budget_too_small_to_fit_falls_back_to_the_undivided_device() {
    // Stands in for `fit_context`: refuses the smaller budget, fits the larger.
    let fit = |budget: Option<u64>| budget.filter(|&b| b >= 6_000).map(|_| 32_768);

    assert_eq!(
        fit_or_undivided(fit, Some(4_000), Some(6_000)),
        Some(32_768),
        "a reservation that refuses must fall through to the undivided device"
    );
}

/// The reserved budget wins whenever it fits — the fallback is an escape
/// hatch, not a preference.
#[test]
fn the_reserved_budget_is_used_whenever_it_fits() {
    let fit = |budget: Option<u64>| budget.map(|b| b / 1000);

    assert_eq!(
        fit_or_undivided(fit, Some(6_000), Some(24_000)),
        Some(6),
        "the undivided device must not be consulted when the reservation fits"
    );
}
