//! The context a launch resolves to, and the fit's fallback.

use super::*;

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

/// A per-request `num_ctx` still outranks the per-model tier — the ordering of
/// the chain (`resolve_context_size`) is unchanged by M9, only who reads the
/// result.
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
