//! Unified process manager for llama-server instances.
//!
//! This module provides a high-level process manager that supports two strategies:
//! - **Concurrent**: Multiple models running simultaneously (GUI use case)
//! - **SingleSwap**: Auto-swapping single model with smart context handling (Proxy use case)

use super::core::GuiProcessCore;
use super::health::wait_for_http_health;
use super::types::ServerInfo;
use anyhow::{Result, anyhow};
use gglib_core::ports::{
    LaunchOverrides, ModelCatalogPort, ModelRuntimeError, ProcessHandle, RunningTarget,
    ServerConfig,
};
use gglib_core::server_config::{CacheRamSetting, ServerConfigOptions};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::process::startup_guard::{
    STARTUP_WAIT_TIMEOUT, StartupDisposition, drive, should_bail_on_insufficient_budget,
    wait_for_startup,
};
use crate::process::swap_state::SwapState;

/// Strategy for managing llama-server processes.
pub enum ProcessStrategy {
    /// Allow multiple concurrent models up to max_concurrent (GUI).
    Concurrent { max_concurrent: usize },
    /// Only allow one model at a time, auto-swap when a different model is
    /// requested (proxy, and `gglib serve` in pinned mode).
    ///
    /// Concurrent requests during startup wait via watch channel instead of
    /// failing immediately. All state and the launch sequence itself live on
    /// [`SwapState`].
    ///
    /// Boxed because `SwapState` carries a full `ServerConfigOptions`
    /// template, which would otherwise inflate every `ProcessStrategy` — and
    /// so every `ProcessManager` — to its size, including in the `Concurrent`
    /// case that has no use for it.
    SingleSwap(Box<SwapState>),
}

/// Unified process manager for llama-server instances.
///
/// Supports two strategies:
/// - **Concurrent**: Multiple models at once (GUI) - use `new_concurrent`
/// - **SingleSwap**: One model at a time, auto-swap (Proxy) - use `new_single_swap`
pub struct ProcessManager {
    core: Arc<RwLock<GuiProcessCore>>,
    strategy: ProcessStrategy,
}

impl ProcessManager {
    /// Create a new `ProcessManager` with Concurrent strategy (for GUI)
    pub fn new_concurrent(
        base_port: u16,
        max_concurrent: usize,
        llama_server_path: impl Into<String>,
    ) -> Self {
        let core = GuiProcessCore::new(base_port, llama_server_path);
        Self {
            core: Arc::new(RwLock::new(core)),
            strategy: ProcessStrategy::Concurrent { max_concurrent },
        }
    }

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
    /// # When to use
    ///
    /// Use `new_single_swap()` when you need a single-model proxy (e.g. the
    /// HTTP API layer). For multi-model workloads (e.g. the GUI dashboard),
    /// prefer [`ProcessManager::new_concurrent`] which allows multiple models
    /// to run simultaneously up to a configurable limit.
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

    /// Start a llama-server instance for a model (Concurrent strategy only)
    pub async fn start_server(&self, config: ServerConfig) -> Result<u16> {
        let max_concurrent = match &self.strategy {
            ProcessStrategy::Concurrent { max_concurrent } => *max_concurrent,
            ProcessStrategy::SingleSwap(_) => {
                return Err(anyhow!(
                    "SingleSwap strategy should use ensure_model_running() instead of start_server()"
                ));
            }
        };

        let model_id = config.model_id as u32;
        let mut core = self.core.write().await;

        // Check if already running
        if core.is_running(model_id) {
            return Err(anyhow!("Model {} is already being served", model_id));
        }

        // Check concurrent limit
        if core.count() >= max_concurrent {
            return Err(anyhow!(
                "Maximum concurrent servers ({}) reached. Stop a server first.",
                max_concurrent
            ));
        }

        // Spawn the process
        let allocated_port = core.spawn(config).await?;

        // Release the lock before waiting
        drop(core);

        // Wait for server to be ready by polling health endpoint
        debug!(port = %allocated_port, "Waiting for llama-server to be ready");
        wait_for_http_health(allocated_port, 30).await?;
        debug!("llama-server is ready and accepting requests");

        Ok(allocated_port)
    }

    /// Ensure a model is running (SingleSwap strategy only).
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
        let state = match &self.strategy {
            ProcessStrategy::SingleSwap(state) => state,
            ProcessStrategy::Concurrent { .. } => {
                return Err(ModelRuntimeError::Internal(
                    "ensure_model_running() is only available for SingleSwap strategy".to_string(),
                ));
            }
        };

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

    /// Get information about the currently running model (SingleSwap only).
    pub async fn current_model(&self) -> Option<RunningTarget> {
        match &self.strategy {
            ProcessStrategy::SingleSwap(state) => {
                let current = Arc::clone(&state.current);
                let guard = current.read().await;
                guard.as_ref().map(|c| {
                    RunningTarget::local(
                        c.port,
                        c.model_id,
                        c.model_name.clone(),
                        c.context_size,
                        false,
                    )
                    .with_slot_restore_supported(c.slot_restore_supported)
                    .with_cache_ram_health(c.cache_ram_health)
                })
            }
            ProcessStrategy::Concurrent { .. } => None,
        }
    }

    /// Stop the currently running model (SingleSwap only).
    pub async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        match &self.strategy {
            ProcessStrategy::SingleSwap(state) => {
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
            ProcessStrategy::Concurrent { .. } => Err(ModelRuntimeError::Internal(
                "stop_current() is only available for SingleSwap strategy".to_string(),
            )),
        }
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
        // For SingleSwap, also clear current model state
        if let ProcessStrategy::SingleSwap(state) = &self.strategy {
            let current = Arc::clone(&state.current);
            let mut guard = current.write().await;
            *guard = None;
        }
        self.stop_all().await
    }

    /// Check if this manager uses SingleSwap strategy.
    #[must_use]
    pub fn is_single_swap(&self) -> bool {
        matches!(self.strategy, ProcessStrategy::SingleSwap(_))
    }

    /// The single model this manager is pinned to, if any.
    ///
    /// `Some(name)` is `gglib serve`: every other model is refused rather
    /// than swapped to. Only `SingleSwap` can be pinned — `Concurrent` serves
    /// many models by design.
    #[must_use]
    pub fn pinned_model(&self) -> Option<&str> {
        match &self.strategy {
            ProcessStrategy::SingleSwap(state) => state.pinned_name(),
            ProcessStrategy::Concurrent { .. } => None,
        }
    }

    /// Check if a model is currently loading (SingleSwap only).
    #[must_use]
    pub fn is_loading(&self) -> bool {
        match &self.strategy {
            ProcessStrategy::SingleSwap(state) => state
                .loading
                .read()
                .ok()
                .map(|s| s.is_some())
                .unwrap_or(false),
            ProcessStrategy::Concurrent { .. } => false,
        }
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

    /// Concurrent serves many models by design and can never be pinned.
    #[test]
    fn concurrent_manager_reports_no_pinned_model() {
        let manager = ProcessManager::new_concurrent(8080, 5, "llama-server");
        assert_eq!(manager.pinned_model(), None);
    }

    #[tokio::test]
    async fn test_concurrent_manager_creation() {
        let manager = ProcessManager::new_concurrent(8080, 5, "llama-server");
        assert_eq!(manager.list_servers().await.len(), 0);
    }

    #[tokio::test]
    async fn test_is_serving() {
        let manager = ProcessManager::new_concurrent(8080, 5, "llama-server");
        assert!(!manager.is_serving(1).await);
    }

    #[tokio::test]
    async fn test_list_servers_empty() {
        let manager = ProcessManager::new_concurrent(8080, 5, "llama-server");
        assert_eq!(manager.list_servers().await.len(), 0);
    }

    #[tokio::test]
    async fn list_running_is_empty_with_no_servers() {
        let manager = ProcessManager::new_concurrent(8080, 5, "llama-server");
        assert!(manager.list_running().await.is_empty());
    }

    /// Both listings project the same underlying process set, so they must
    /// never disagree on how many servers are up.
    #[tokio::test]
    async fn list_running_agrees_with_list_servers() {
        let manager = ProcessManager::new_concurrent(8080, 5, "llama-server");
        assert_eq!(
            manager.list_running().await.len(),
            manager.list_servers().await.len()
        );
    }

    #[tokio::test]
    async fn test_is_single_swap() {
        let manager = ProcessManager::new_concurrent(8080, 5, "llama-server");
        assert!(!manager.is_single_swap());
    }

    #[tokio::test]
    async fn test_is_loading_concurrent() {
        let manager = ProcessManager::new_concurrent(8080, 5, "llama-server");
        assert!(!manager.is_loading());
    }
}
