//! Unified process manager for llama-server instances.
//!
//! Every launch surface — the CLI, the proxy, both GUIs — shares one manager
//! (built once by `build_service_graph`), which is what makes "gglib owns every
//! llama-server on this machine" an invariant rather than a hope.
//!
//! What that manager guarantees changed with M9. It used to be *one* model at a
//! time, enforced by killing whatever was loaded before spawning anything else.
//! It is now a bounded resident set — see
//! [`admission`](crate::process::admission) for how many, and why — with a
//! queue deciding who occupies it. The dispatch here is unchanged in shape:
//! this type routes, and [`ResidentSet`] owns both the state and the launch
//! sequence that mutates it.

use super::core::GuiProcessCore;
use super::types::ServerInfo;
use anyhow::Result;
use gglib_core::domain::AdmissionSnapshot;
use gglib_core::ports::{
    Admission, LaunchOverrides, ModelCatalogPort, ModelRuntimeError, ProcessHandle, RunningTarget,
};
use gglib_core::server_config::{CacheRamSetting, ServerConfigOptions};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::process::residency::ResidentSet;

/// Unified process manager for llama-server instances.
///
/// Wraps a [`ResidentSet`] — the VRAM slots and the admission queue that fills
/// them — with the process-level queries the GUI and CLI need.
pub struct ProcessManager {
    core: Arc<RwLock<GuiProcessCore>>,
    residency: ResidentSet,
}

impl ProcessManager {
    /// Create a new `ProcessManager`.
    ///
    /// Requests are admitted through a queue rather than racing each other: a
    /// request for a model that is already resident is served immediately,
    /// while one for a model that is not waits until it can take a VRAM slot —
    /// either alongside what is loaded, or by displacing it once nothing is
    /// being served from it. See [`crate::process::admission`].
    ///
    /// # Arguments
    ///
    /// * `base_port` — Base port for llama-server allocation. Ports are
    ///   assigned sequentially starting from this value.
    /// * `llama_server_path` — Path to the llama-server binary to execute.
    /// * `catalog` — Model catalog used to resolve model names into launch
    ///   specifications (file paths, context sizes, etc.).
    /// * `launch_overrides` — Standing launch options every spawn starts from
    ///   (slot-save path, cache reuse, KV cache types, and anything else
    ///   [`ServerConfigOptions`] carries). Per-call [`LaunchOverrides`] are
    ///   layered on top; see [`ResidentSet`] for the composition order.
    /// * `cache_ram` — how to size llama-server's own host-RAM prompt cache
    ///   (`--cache-ram`). Not part of `launch_overrides` because it is resolved
    ///   at spawn rather than passed through.
    ///   [`CacheRamSetting::ExplicitMb`]`(0)` disables the cache outright — the
    ///   right choice for benchmark launches, where a prompt cache would
    ///   perturb prefill timings. (Omitting the flag entirely, so llama-server's
    ///   own default applies, is what [`CacheRamSetting::Auto`] does when
    ///   autosizing is suppressed by env.)
    ///
    /// Use [`Self::set_pin`] afterwards when the manager must refuse every
    /// model but one (`gglib serve`).
    pub fn new(
        base_port: u16,
        llama_server_path: impl Into<String>,
        catalog: Arc<dyn ModelCatalogPort>,
        launch_overrides: ServerConfigOptions,
        cache_ram: CacheRamSetting,
    ) -> Self {
        let core = GuiProcessCore::new(base_port, llama_server_path);
        Self {
            core: Arc::new(RwLock::new(core)),
            residency: ResidentSet::new(catalog, launch_overrides, cache_ram),
        }
    }

    /// Pin this manager to a single model, or clear the pin (`gglib serve`).
    ///
    /// While pinned, the manager behaves exactly like the unpinned one for the
    /// pinned model — same admission, same cache handling, same launch options
    /// template, with the pin's own overrides layered on top — but rejects
    /// every other model with [`ModelRuntimeError::PinnedModelMismatch`]
    /// instead of admitting it.
    ///
    /// That refusal is the feature. `gglib serve <model>` exists to give
    /// single-model clients (VS Code Copilot's BYOK endpoint, for one) an
    /// endpoint that cannot change model underneath them; silently honouring a
    /// foreign request would defeat the guarantee they are relying on.
    ///
    /// Runtime-mutable rather than a constructor because the daemon owns one
    /// long-lived manager: the pin is applied when a pinned proxy run starts
    /// and cleared when it stops.
    pub fn set_pin(&self, pin: Option<gglib_core::ports::PinnedSpec>) {
        self.residency.set_pin(pin);
    }

    /// Admit a request to a running model.
    ///
    /// The returned [`Admission::lease`] must be held for as long as the
    /// request is being served — it is what tells the queue the model is still
    /// in use and must not be swapped out. See
    /// [`ModelRuntimePort::admit`](gglib_core::ports::ModelRuntimePort::admit).
    ///
    /// # Errors
    ///
    /// Returns `ModelRuntimeError` if the model cannot be started, or
    /// [`ModelRuntimeError::AdmissionTimeout`] if the request never reached the
    /// front of the queue.
    ///
    /// # Known limitations
    ///
    /// If a displaced model's shutdown timed out (D-state process), the
    /// subsequent spawn may fail with a port-in-use or CUDA OOM error. There is
    /// no automatic retry — the caller receives the error and must retry
    /// manually.
    pub async fn admit(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: u64,
        overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        self.residency
            .admit(&self.core, model_name, num_ctx, default_ctx, overrides)
            .await
    }

    /// What the admission queue and resident set look like right now.
    #[must_use]
    pub fn admission_snapshot(&self) -> AdmissionSnapshot {
        self.residency.queue().snapshot()
    }

    /// Get information about the model in the primary slot.
    ///
    /// The primary is the slot chat traffic follows; a co-resident auxiliary
    /// model is deliberately not reported here, because every caller of this
    /// method — the `/slots` poller, the GUI's running-model panel — means "the
    /// model this endpoint is serving".
    pub fn current_model(&self) -> Option<RunningTarget> {
        self.residency.current_model()
    }

    /// Stop the model in the primary slot.
    ///
    /// # Errors
    ///
    /// Returns `ModelRuntimeError` if the process could not be stopped.
    pub async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        self.residency.stop_primary(&self.core).await
    }

    /// Stop a running server by model ID
    ///
    /// # Errors
    ///
    /// Returns an error if the process could not be stopped.
    pub async fn stop_server(&self, model_id: u32) -> Result<()> {
        let mut core = self.core.write().await;
        core.kill(model_id).await
    }

    /// Stop all running servers
    ///
    /// # Errors
    ///
    /// Never returns an error; individual failures are logged.
    pub async fn stop_all(&self) -> Result<()> {
        let mut core = self.core.write().await;
        core.kill_all().await;
        Ok(())
    }

    /// Check if a model is being served
    pub async fn is_serving(&self, model_id: u32) -> bool {
        let core = self.core.read().await;
        core.is_running(model_id)
    }

    /// Get info for a running server
    pub async fn get_server_info(&self, model_id: u32) -> Option<ServerInfo> {
        let core = self.core.read().await;
        core.get_info(model_id).cloned()
    }

    /// List all running servers
    pub async fn list_servers(&self) -> Vec<ServerInfo> {
        let core = self.core.read().await;
        core.list_all().into_iter().cloned().collect()
    }

    /// List running servers as [`ProcessHandle`]s.
    ///
    /// The same processes [`Self::list_servers`] reports, projected onto the
    /// port type so callers that already speak `ProcessHandle` — the GUI
    /// server list and its health monitor — can consume a manager-backed
    /// runtime without a second shape to handle.
    pub async fn list_running(&self) -> Vec<ProcessHandle> {
        let core = self.core.read().await;
        core.list_all()
            .into_iter()
            .map(|info| {
                ProcessHandle::new(
                    i64::from(info.model_id),
                    info.model_name.clone(),
                    Some(info.pid),
                    info.port,
                    info.started_at,
                )
            })
            .collect()
    }

    /// Graceful shutdown
    ///
    /// # Errors
    ///
    /// Never returns an error; individual failures are logged.
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down process manager");
        self.residency.forget_all();
        self.stop_all().await
    }

    /// The single model this manager is pinned to, if any.
    ///
    /// `Some(name)` is `gglib serve`: every other model is refused rather
    /// than admitted. Owned because the pin is runtime-mutable state behind a
    /// lock (see [`Self::set_pin`]).
    #[must_use]
    pub fn pinned_model(&self) -> Option<String> {
        self.residency.pinned_name()
    }

    /// Check if any slot is mid-launch.
    #[must_use]
    pub fn is_loading(&self) -> bool {
        self.residency.queue().is_loading()
    }
}

// Note: ProcessManager is not Clone because ResidentSet contains
// Arc<dyn ...> and RwLock which don't trivially clone in a meaningful way.
// If you need shared access, wrap ProcessManager in Arc.

#[cfg(test)]
mod tests {
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
        ) -> Result<Option<gglib_core::ports::ModelSummary>, gglib_core::ports::CatalogError>
        {
            Ok(None)
        }
        async fn resolve_for_launch(
            &self,
            _name: &str,
        ) -> Result<Option<gglib_core::ports::ModelLaunchSpec>, gglib_core::ports::CatalogError>
        {
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
            .admit("llama-3-8b", None, 4096, LaunchOverrides::default())
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
    /// to `None`, so reaching the catalog would surface as ModelNotFound.
    #[tokio::test]
    async fn foreign_model_is_refused_before_catalog_lookup() {
        let err = pinned_manager()
            .admit("llama-3-8b", None, 4096, LaunchOverrides::default())
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
            .admit("qwen2.5", None, 4096, LaunchOverrides::default())
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
            .admit("anything", None, 4096, LaunchOverrides::default())
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
            .admit("nope", None, 4096, LaunchOverrides::default())
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
    async fn test_is_serving() {
        assert!(!manager().is_serving(1).await);
    }

    #[tokio::test]
    async fn test_list_servers_empty() {
        assert_eq!(manager().list_servers().await.len(), 0);
    }

    #[tokio::test]
    async fn list_running_is_empty_with_no_servers() {
        assert!(manager().list_running().await.is_empty());
    }

    /// Both listings project the same underlying process set, so they must
    /// never disagree on how many servers are up.
    #[tokio::test]
    async fn list_running_agrees_with_list_servers() {
        let manager = manager();
        assert_eq!(
            manager.list_running().await.len(),
            manager.list_servers().await.len()
        );
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
}
