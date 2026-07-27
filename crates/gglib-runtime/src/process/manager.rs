//! Unified process manager for llama-server instances.
//!
//! Every launch surface — the CLI, the proxy, both GUIs — now shares one
//! `SingleSwap` manager (built once by `build_service_graph`), which is what
//! makes "only one llama-server runs at a time system-wide" an invariant
//! rather than a hope. A `Concurrent` strategy existed here for the GUI's
//! earlier direct-spawn path; epic #630 routed the GUI through the proxy's
//! manager instead, so it was deleted along with the rest of that path.

use super::core::GuiProcessCore;
use super::types::ServerInfo;
use anyhow::Result;
use gglib_core::ports::{
    LaunchOverrides, ModelCatalogPort, ModelRuntimeError, ProcessHandle, RunningTarget,
};
use gglib_core::server_config::{CacheRamSetting, ServerConfigOptions};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::info;

use crate::process::startup_guard::{
    STARTUP_WAIT_TIMEOUT, StartupDisposition, drive, should_bail_on_insufficient_budget,
    wait_for_startup,
};
use crate::process::swap_state::SwapState;

/// Strategy for managing llama-server processes.
///
/// The only strategy today: auto-swap a single model at a time (proxy, GUI,
/// and `gglib serve` in pinned mode). Kept as an enum — rather than folding
/// `SwapState` directly into `ProcessManager` — so a future strategy remains
/// a variant away rather than a structural change.
pub enum ProcessStrategy {
    /// Only allow one model at a time, auto-swap when a different model is
    /// requested.
    ///
    /// Concurrent requests during startup wait via watch channel instead of
    /// failing immediately. All state and the launch sequence itself live on
    /// [`SwapState`].
    ///
    /// Boxed because `SwapState` carries a full `ServerConfigOptions`
    /// template, which would otherwise inflate every `ProcessManager` to its
    /// size even before any model has launched.
    SingleSwap(Box<SwapState>),
}

/// Unified process manager for llama-server instances.
///
/// One strategy today — `SingleSwap`, one model at a time, auto-swap — built
/// via `new_single_swap` or `new_pinned`. See [`ProcessStrategy`].
pub struct ProcessManager {
    core: Arc<RwLock<GuiProcessCore>>,
    strategy: ProcessStrategy,
}

impl ProcessManager {
    /// Create a new `ProcessManager` with SingleSwap strategy (for Proxy).
    ///
    /// This strategy allows only one model to run at a time. When a request
    /// arrives for a different model, the currently running server is stopped
    /// and replaced ("swapped") with the newly requested model.
    ///
    /// Concurrent startup requests are coordinated via watch channels: if
    /// multiple callers simultaneously request the same model while it is
    /// starting up, only one drives the launch; the others subscribe to a
    /// shared channel and receive the result when the driver completes. This
    /// prevents port conflicts and redundant health checks.
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
    ///   [`ServerConfigOptions`] carries). Per-call
    ///   [`LaunchOverrides`] are layered on top; see
    ///   [`SwapState`](crate::process::swap_state::SwapState) for the
    ///   composition order.
    /// * `cache_ram` — how to size llama-server's own host-RAM prompt cache
    ///   (`--cache-ram`). Not part of `launch_overrides` because it is
    ///   resolved at spawn rather than passed through.
    ///   [`CacheRamSetting::LlamaDefault`] emits no flag — the right choice for
    ///   benchmark launches, where a large prompt cache would perturb results.
    ///
    /// Use [`Self::new_pinned`] instead when the manager must refuse every
    /// model but one.
    pub fn new_single_swap(
        base_port: u16,
        llama_server_path: impl Into<String>,
        catalog: Arc<dyn ModelCatalogPort>,
        launch_overrides: ServerConfigOptions,
        cache_ram: CacheRamSetting,
    ) -> Self {
        let core = GuiProcessCore::new(base_port, llama_server_path);
        Self {
            core: Arc::new(RwLock::new(core)),
            strategy: ProcessStrategy::SingleSwap(Box::new(SwapState::new(
                catalog,
                launch_overrides,
                cache_ram,
            ))),
        }
    }

    /// Create a `ProcessManager` pinned to a single model (`gglib serve`).
    ///
    /// Behaves exactly like [`Self::new_single_swap`] for the pinned model —
    /// same startup coordination, same cache handling, same launch options
    /// template — but rejects every other model with
    /// [`ModelRuntimeError::PinnedModelMismatch`] instead of swapping to it.
    ///
    /// That refusal is the feature. `gglib serve <model>` exists to give
    /// single-model clients (VS Code Copilot's BYOK endpoint, for one) an
    /// endpoint that cannot change model underneath them; silently honouring a
    /// foreign request would defeat the guarantee they are relying on.
    ///
    /// # Arguments
    ///
    /// Identical to [`Self::new_single_swap`], plus `model_name` — the model
    /// to pin to, matched exactly against each request's model name.
    pub fn new_pinned(
        model_name: impl Into<String>,
        base_port: u16,
        llama_server_path: impl Into<String>,
        catalog: Arc<dyn ModelCatalogPort>,
        launch_overrides: ServerConfigOptions,
        cache_ram: CacheRamSetting,
    ) -> Self {
        let core = GuiProcessCore::new(base_port, llama_server_path);
        Self {
            core: Arc::new(RwLock::new(core)),
            strategy: ProcessStrategy::SingleSwap(Box::new(SwapState::pinned_to(
                model_name,
                catalog,
                launch_overrides,
                cache_ram,
            ))),
        }
    }

    /// Ensure a model is running.
    ///
    /// This method:
    /// 1. Atomically checks if another startup is in progress (via watch channel)
    /// 2. If waiting, subscribes to the existing driver's result
    /// 3. If initiating, spawns a detached driver task and waits for its result
    /// 4. All callers — including the initiator — wait on the same watch channel,
    ///    so one client disconnecting does not fail other concurrent requests
    ///
    /// # Errors
    ///
    /// Returns `ModelRuntimeError` if the model cannot be started.
    ///
    /// # Known limitations
    ///
    /// If a previous model's shutdown timed out (D-state process), the subsequent spawn
    /// may fail with a port-in-use or CUDA OOM error. There is no automatic retry — the
    /// caller receives the error and must retry manually. GPU memory availability is not
    /// checked before spawn; failures surface as generic CUDA OOM rather than an
    /// actionable "previous process may still hold resources" message.
    pub async fn ensure_model_running(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        self.ensure_model_running_with(model_name, num_ctx, default_ctx, LaunchOverrides::default())
            .await
    }

    /// Same as [`Self::ensure_model_running`], but with per-call launch
    /// overrides layered on the manager's standing template.
    ///
    /// Lets a single shared `ProcessManager` serve callers with different
    /// launch needs — a GUI start request carrying `--mlock`, and a benchmark
    /// runner that must never gain a prompt cache — without constructing a
    /// second manager and losing the one-llama-server-at-a-time guarantee.
    /// [`LaunchOverrides::default`] means "no opinion": every field falls
    /// through to what the manager was constructed with (see
    /// [`Self::new_single_swap`]).
    ///
    /// # Errors
    ///
    /// Returns `ModelRuntimeError` if the model cannot be started.
    pub async fn ensure_model_running_with(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: u64,
        overrides: LaunchOverrides,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        let ProcessStrategy::SingleSwap(state) = &self.strategy;

        // Refuse foreign models before touching the startup guard, so a
        // rejected request neither queues behind the pinned model nor
        // displaces it.
        state.check_pinned(model_name)?;

        // Retry loop with an overall deadline, so a caller cannot wait
        // unboundedly while other models swap in and out ahead of it.
        let deadline = tokio::time::Instant::now() + STARTUP_WAIT_TIMEOUT;

        loop {
            match StartupDisposition::check(&state.loading, model_name.to_string()) {
                StartupDisposition::Waiter {
                    rx,
                    target_model_name,
                } => {
                    if target_model_name == model_name {
                        // Our model is already starting — wait for that result.
                        // Offset by 5s so the driver always broadcasts first.
                        return wait_for_startup(rx, STARTUP_WAIT_TIMEOUT + Duration::from_secs(5))
                            .await;
                    }
                    // A different model is starting. Wait for it to finish, then retry.
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    if should_bail_on_insufficient_budget(remaining) {
                        return Err(ModelRuntimeError::ContentionTimeout(
                            "Insufficient time remaining for model startup after contention"
                                .to_string(),
                        ));
                    }
                    let _ = wait_for_startup(rx, remaining).await;
                }
                StartupDisposition::Initiator { guard, self_rx } => {
                    drive(
                        guard,
                        STARTUP_WAIT_TIMEOUT,
                        state.startup_future(
                            Arc::clone(&self.core),
                            model_name.to_string(),
                            num_ctx,
                            default_ctx,
                            overrides,
                        ),
                    );

                    // Wait on the same channel as every other caller.
                    return wait_for_startup(
                        self_rx,
                        STARTUP_WAIT_TIMEOUT + Duration::from_secs(5),
                    )
                    .await;
                }
            }
        }
    } // end ensure_model_running

    /// Get information about the currently running model.
    pub async fn current_model(&self) -> Option<RunningTarget> {
        let ProcessStrategy::SingleSwap(state) = &self.strategy;
        let current = Arc::clone(&state.current);
        let guard = current.read().await;
        guard.as_ref().map(|c| {
            RunningTarget::local(c.port, c.model_id, c.model_name.clone(), c.context_size, false)
                .with_slot_restore_supported(c.slot_restore_supported)
                .with_cache_ram_health(c.cache_ram_health)
        })
    }

    /// Stop the currently running model.
    pub async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        let ProcessStrategy::SingleSwap(state) = &self.strategy;
        let current = Arc::clone(&state.current);
        let mut guard = current.write().await;
        if let Some(state) = guard.take() {
            let mut core = self.core.write().await;
            core.kill(state.model_id)
                .await
                .map_err(|e| ModelRuntimeError::Internal(e.to_string()))?;
        }
        Ok(())
    }

    /// Stop a running server by model ID
    pub async fn stop_server(&self, model_id: u32) -> Result<()> {
        let mut core = self.core.write().await;
        core.kill(model_id).await
    }

    /// Stop all running servers
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
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down process manager");
        let ProcessStrategy::SingleSwap(state) = &self.strategy;
        let current = Arc::clone(&state.current);
        let mut guard = current.write().await;
        *guard = None;
        drop(guard);
        self.stop_all().await
    }

    /// The single model this manager is pinned to, if any.
    ///
    /// `Some(name)` is `gglib serve`: every other model is refused rather
    /// than swapped to.
    #[must_use]
    pub fn pinned_model(&self) -> Option<&str> {
        let ProcessStrategy::SingleSwap(state) = &self.strategy;
        state.pinned_name()
    }

    /// Check if a model is currently loading.
    #[must_use]
    pub fn is_loading(&self) -> bool {
        let ProcessStrategy::SingleSwap(state) = &self.strategy;
        state.loading.read().ok().map(|s| s.is_some()).unwrap_or(false)
    }
}

// Note: ProcessManager is not Clone because ProcessStrategy contains
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

    fn pinned_manager() -> ProcessManager {
        ProcessManager::new_pinned(
            "qwen2.5",
            9000,
            "llama-server",
            Arc::new(StubCatalog),
            ServerConfigOptions::default(),
            CacheRamSetting::Auto,
        )
    }

    /// The guard has to sit on the real entry point, not just on SwapState —
    /// this is what a proxy request actually calls.
    #[tokio::test]
    async fn ensure_model_running_rejects_a_foreign_model() {
        let err = pinned_manager()
            .ensure_model_running("llama-3-8b", None, 4096)
            .await
            .expect_err("a pinned manager must refuse a foreign model");

        assert!(
            matches!(err, ModelRuntimeError::PinnedModelMismatch { .. }),
            "expected PinnedModelMismatch, got {err:?}"
        );
    }

    /// A foreign request must be refused without the catalog ever being
    /// consulted, proving it short-circuits ahead of the startup machinery
    /// rather than failing somewhere inside it. The stub resolves every model
    /// to `None`, so reaching the catalog would surface as ModelNotFound.
    #[tokio::test]
    async fn foreign_model_is_refused_before_catalog_lookup() {
        let err = pinned_manager()
            .ensure_model_running("llama-3-8b", None, 4096)
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
    async fn ensure_model_running_admits_the_pinned_model() {
        let err = pinned_manager()
            .ensure_model_running("qwen2.5", None, 4096)
            .await
            .unwrap_err();

        assert!(
            matches!(err, ModelRuntimeError::ModelNotFound(_)),
            "pinned model should pass the guard and reach the catalog, got {err:?}"
        );
    }

    /// Pinning must not leak into the ordinary proxy manager.
    #[tokio::test]
    async fn single_swap_manager_admits_any_model() {
        let manager = ProcessManager::new_single_swap(
            9000,
            "llama-server",
            Arc::new(StubCatalog),
            ServerConfigOptions::default(),
            CacheRamSetting::Auto,
        );

        let err = manager
            .ensure_model_running("anything", None, 4096)
            .await
            .unwrap_err();

        assert!(
            matches!(err, ModelRuntimeError::ModelNotFound(_)),
            "unpinned manager must not reject on identity, got {err:?}"
        );
    }

    /// The read side of the guard: callers that want to avoid provoking a
    /// mismatch — `/v1/models`, which should not advertise a model that can
    /// only be refused — need the name without attempting a request.
    #[test]
    fn pinned_manager_reports_its_model() {
        assert_eq!(pinned_manager().pinned_model(), Some("qwen2.5"));
    }

    /// Reporting must agree with admission: a manager that admits any model
    /// must not name one, or callers would narrow what they offer for no
    /// reason.
    #[test]
    fn single_swap_manager_reports_no_pinned_model() {
        let manager = ProcessManager::new_single_swap(
            9000,
            "llama-server",
            Arc::new(StubCatalog),
            ServerConfigOptions::default(),
            CacheRamSetting::Auto,
        );

        assert_eq!(manager.pinned_model(), None);
    }

    fn single_swap_manager() -> ProcessManager {
        ProcessManager::new_single_swap(
            9000,
            "llama-server",
            Arc::new(StubCatalog),
            ServerConfigOptions::default(),
            CacheRamSetting::Auto,
        )
    }

    #[tokio::test]
    async fn test_is_serving() {
        let manager = single_swap_manager();
        assert!(!manager.is_serving(1).await);
    }

    #[tokio::test]
    async fn test_list_servers_empty() {
        let manager = single_swap_manager();
        assert_eq!(manager.list_servers().await.len(), 0);
    }

    #[tokio::test]
    async fn list_running_is_empty_with_no_servers() {
        let manager = single_swap_manager();
        assert!(manager.list_running().await.is_empty());
    }

    /// Both listings project the same underlying process set, so they must
    /// never disagree on how many servers are up.
    #[tokio::test]
    async fn list_running_agrees_with_list_servers() {
        let manager = single_swap_manager();
        assert_eq!(
            manager.list_running().await.len(),
            manager.list_servers().await.len()
        );
    }

    #[tokio::test]
    async fn test_is_loading() {
        let manager = single_swap_manager();
        assert!(!manager.is_loading());
    }
}
