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

/// Two models, found by id or by exact name as the real catalog finds them:
/// `qwen2.5` is id 3 and `llama-3-8b` is id 7. Neither file exists, so an
/// admission that gets past the pin and the queue stops at the launch.
#[derive(Debug)]
pub(in crate::process) struct StubCatalog;

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
        name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok([(3, "qwen2.5"), (7, "llama-3-8b")]
            .into_iter()
            .find(|(id, model)| id.to_string() == name || *model == name)
            .map(|(id, model)| launch_spec(id, model)))
    }
}

/// Holds one model, whose launch specification is `self.0`, and answers
/// every name with it.
#[derive(Debug)]
pub(in crate::process) struct OneModel(pub ModelLaunchSpec);

#[async_trait]
impl ModelCatalogPort for OneModel {
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
        Ok(Some(self.0.clone()))
    }
}

/// A launch spec for model `id` named `name`, whose file does not exist.
pub(in crate::process) fn launch_spec(id: u32, name: &str) -> ModelLaunchSpec {
    ModelLaunchSpec {
        model_sampling: gglib_core::domain::ModelSamplingDefaults::default(),
        id,
        name: name.to_owned(),
        file_path: format!("/nonexistent/{name}.gguf").into(),
        projector: None,
        tags: Vec::new(),
        architecture: None,
        quantization: None,
        context_length: None,
        server_defaults: None,
        file_size_bytes: 0,
        kv_elems_per_token: None,
        kv_memory_is_partial: false,
    }
}

/// A pin on model `id` named `name`, with no launch overrides of its own.
pub(in crate::process) fn pin(id: i64, name: &str) -> PinnedSpec {
    PinnedSpec {
        id,
        name: name.to_owned(),
        launch_overrides: ServerConfigOptions::default(),
    }
}

fn swapping_set() -> ResidentSet {
    ResidentSet::new(
        Arc::new(StubCatalog),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    )
}

fn pinned_set(id: i64, name: &str) -> ResidentSet {
    let set = swapping_set();
    set.set_pin(Some(pin(id, name)));
    set
}

// ── the pin ───────────────────────────────────────────────────────────────

#[test]
fn pinned_state_admits_its_own_model() {
    let set = pinned_set(3, "qwen2.5");
    assert!(set.check_pinned(&launch_spec(3, "qwen2.5")).is_ok());
}

#[test]
fn pinned_state_rejects_a_foreign_model() {
    let err = pinned_set(3, "qwen2.5")
        .check_pinned(&launch_spec(7, "llama-3-8b"))
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

/// Matching is by id: a pinned endpoint must not serve a different model
/// because it shares the pinned one's name, and its own model stays its own
/// under whatever name the catalog now gives it.
#[test]
fn pinned_matching_is_by_id() {
    let set = pinned_set(3, "qwen2.5");
    assert!(
        set.check_pinned(&launch_spec(7, "qwen2.5")).is_err(),
        "another model with the same name"
    );
    assert!(
        set.check_pinned(&launch_spec(3, "qwen2.5-renamed")).is_ok(),
        "the pinned model, renamed"
    );
}

/// The unpinned proxy must keep admitting freely — pinning is opt-in.
#[test]
fn unpinned_state_admits_any_model() {
    let set = swapping_set();
    assert!(set.check_pinned(&launch_spec(3, "qwen2.5")).is_ok());
    assert!(set.check_pinned(&launch_spec(7, "llama-3-8b")).is_ok());
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
    set.set_pin(Some(pin(3, "qwen2.5")));

    assert_eq!(set.launch_overrides.mlock, template.mlock);
    assert_eq!(set.launch_overrides.cache_reuse, template.cache_reuse);
    assert_eq!(set.cache_ram, CacheRamSetting::ExplicitMb(4096));
}

/// Clearing the pin restores ordinary auto-swapping admission.
#[test]
fn clearing_the_pin_restores_auto_swapping() {
    let set = pinned_set(3, "qwen2.5");
    assert!(set.check_pinned(&launch_spec(7, "llama-3-8b")).is_err());

    set.set_pin(None);

    assert!(set.check_pinned(&launch_spec(7, "llama-3-8b")).is_ok());
    assert!(set.pinned().is_none());
}

/// Re-pinning replaces the previous pin rather than accumulating.
#[test]
fn repinning_replaces_the_previous_pin() {
    let set = pinned_set(3, "qwen2.5");
    set.set_pin(Some(pin(7, "llama-3-8b")));

    assert!(set.check_pinned(&launch_spec(7, "llama-3-8b")).is_ok());
    assert!(set.check_pinned(&launch_spec(3, "qwen2.5")).is_err());
}

// ── a fresh set ───────────────────────────────────────────────────────────

#[test]
fn a_fresh_set_is_empty() {
    let set = swapping_set();
    assert!(set.current_model().is_none());
    assert!(!set.queue().is_loading());
    assert!(set.queue().snapshot().slots.is_empty());
}
