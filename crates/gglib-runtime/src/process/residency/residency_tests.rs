//! Resident-set behaviour that does not need a llama-server.
//!
//! The launch sequence itself is untestable without spawning a process, so what
//! is exercised here is everything around it: the pin guard and a fresh set.
//! The context resolution every admission depends on, and the fit's fallback,
//! are tested beside `context.rs`; the scheduling rules live next door in
//! `admission`, and are tested there.

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
