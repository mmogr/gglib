//! `ProcessManager`: the pin, and what a fresh manager reports.

use super::*;

// ---------------------------------------------------------------
// Pinned mode
// ---------------------------------------------------------------

#[derive(Debug)]
struct StubCatalog;

#[async_trait::async_trait]
impl ModelCatalogPort for StubCatalog {
    async fn list_models(
        &self,
    ) -> Result<Vec<gglib_core::ports::ModelSummary>, gglib_core::ports::CatalogError> {
        Ok(Vec::new())
    }
    async fn resolve_model(
        &self,
        _name: &str,
    ) -> Result<Option<gglib_core::ports::ModelSummary>, gglib_core::ports::CatalogError> {
        Ok(None)
    }
    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<gglib_core::ports::ModelLaunchSpec>, gglib_core::ports::CatalogError> {
        Ok(None)
    }
}

fn manager() -> ProcessManager {
    ProcessManager::new(
        9000,
        "llama-server",
        Arc::new(StubCatalog),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    )
}

fn pinned_manager() -> ProcessManager {
    let manager = manager();
    manager.set_pin(Some(gglib_core::ports::PinnedSpec {
        name: "qwen2.5".to_string(),
        launch_overrides: ServerConfigOptions::default(),
    }));
    manager
}

/// The guard has to sit on the real entry point, not just on `ResidentSet` —
/// this is what a proxy request actually calls.
#[tokio::test]
async fn admit_rejects_a_foreign_model() {
    let err = pinned_manager()
        .admit("llama-3-8b", None, Some(4096), LaunchOverrides::default())
        .await
        .expect_err("a pinned manager must refuse a foreign model");

    assert!(
        matches!(err, ModelRuntimeError::PinnedModelMismatch { .. }),
        "expected PinnedModelMismatch, got {err:?}"
    );
}

/// A foreign request must be refused without the catalog ever being
/// consulted, proving it short-circuits ahead of the admission machinery
/// rather than failing somewhere inside it. The stub resolves every model
/// to `None`, so reaching the catalog would surface as `ModelNotFound`.
#[tokio::test]
async fn foreign_model_is_refused_before_catalog_lookup() {
    let err = pinned_manager()
        .admit("llama-3-8b", None, Some(4096), LaunchOverrides::default())
        .await
        .unwrap_err();

    assert!(
        !matches!(err, ModelRuntimeError::ModelNotFound(_)),
        "request reached the catalog instead of being refused up front"
    );
}

/// The pinned model itself is admitted past the guard — it fails later,
/// at catalog resolution, which is exactly how far this stub allows.
#[tokio::test]
async fn admit_allows_the_pinned_model_through_to_the_catalog() {
    let err = pinned_manager()
        .admit("qwen2.5", None, Some(4096), LaunchOverrides::default())
        .await
        .unwrap_err();

    assert!(
        matches!(err, ModelRuntimeError::ModelNotFound(_)),
        "pinned model should pass the guard and reach the catalog, got {err:?}"
    );
}

/// Pinning must not leak into the ordinary proxy manager.
#[tokio::test]
async fn an_unpinned_manager_admits_any_model() {
    let err = manager()
        .admit("anything", None, Some(4096), LaunchOverrides::default())
        .await
        .unwrap_err();

    assert!(
        matches!(err, ModelRuntimeError::ModelNotFound(_)),
        "unpinned manager must not reject on identity, got {err:?}"
    );
}

/// An unknown model must fail immediately rather than joining the queue and
/// waiting out a swap only to discover nobody has it.
#[tokio::test]
async fn an_unknown_model_fails_without_queueing() {
    let manager = manager();
    let _ = manager
        .admit("nope", None, Some(4096), LaunchOverrides::default())
        .await;

    let snapshot = manager.admission_snapshot();
    assert_eq!(snapshot.waiting(), 0, "nothing should be left queued");
    assert_eq!(snapshot.total_swaps, 0);
}

/// The read side of the guard: callers that want to avoid provoking a
/// mismatch — `/v1/models`, which should not advertise a model that can
/// only be refused — need the name without attempting a request.
#[test]
fn pinned_manager_reports_its_model() {
    assert_eq!(pinned_manager().pinned_model().as_deref(), Some("qwen2.5"));
}

/// Reporting must agree with admission: a manager that admits any model
/// must not name one, or callers would narrow what they offer for no
/// reason.
#[test]
fn an_unpinned_manager_reports_no_pinned_model() {
    assert_eq!(manager().pinned_model(), None);
}

#[tokio::test]
async fn list_running_is_empty_with_no_servers() {
    assert!(manager().list_running().await.is_empty());
}

#[tokio::test]
async fn test_is_loading() {
    assert!(!manager().is_loading());
}

/// A fresh manager holds nothing and has done nothing.
#[test]
fn a_fresh_manager_reports_an_empty_resident_set() {
    let snapshot = manager().admission_snapshot();
    assert!(snapshot.slots.is_empty());
    assert!(snapshot.queued.is_empty());
    assert_eq!(snapshot.total_swaps, 0);
    assert_eq!(snapshot.secondary_slot.state, "available");
}

#[test]
fn a_fresh_manager_has_no_current_model() {
    assert!(manager().current_model().is_none());
}
