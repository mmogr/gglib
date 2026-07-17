//! Unified process manager for llama-server instances.
//!
//! This module provides a high-level process manager that supports two strategies:
//! - **Concurrent**: Multiple models running simultaneously (GUI use case)
//! - **SingleSwap**: Auto-swapping single model with smart context handling (Proxy use case)

use super::core::GuiProcessCore;
use super::health::{check_http_health, wait_for_http_health};
use super::types::ServerInfo;
use anyhow::{Result, anyhow};
use gglib_core::ports::{
    CatalogError, ModelCatalogPort, ModelRuntimeError, RunningTarget, ServerConfig,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::process::startup_guard::{
    STARTUP_WAIT_TIMEOUT, StartupDisposition, drive, should_bail_on_insufficient_budget,
    wait_for_startup,
};
use crate::server_config::{ServerConfigOptions, build_server_config};

/// Currently running model state for SingleSwap strategy.
#[derive(Debug, Clone)]
pub struct CurrentModelState {
    /// Database ID of the running model.
    pub model_id: u32,
    /// Model name.
    pub model_name: String,
    /// Context size being used.
    pub context_size: u64,
    /// Port the server is listening on.
    pub port: u16,
    /// Path to the model file.
    pub model_path: PathBuf,
}

/// Strategy for managing llama-server processes.
pub enum ProcessStrategy {
    /// Allow multiple concurrent models up to max_concurrent (GUI).
    Concurrent { max_concurrent: usize },
    /// Only allow one model at a time, auto-swap when different model requested (Proxy).
    /// Concurrent requests during startup wait via watch channel instead of failing immediately.
    SingleSwap {
        /// Model catalog for resolving model names and getting launch specs.
        catalog: Arc<dyn ModelCatalogPort>,
        /// Currently running model state (Arc for 'static spawn compatibility).
        current: Arc<RwLock<Option<CurrentModelState>>>,
        /// Loading slot — `Some(StartupState)` means a driver is active, `None` means idle.
        loading: Arc<std::sync::RwLock<Option<crate::process::startup_guard::StartupState>>>,
    },
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
    ) -> Self {
        let core = GuiProcessCore::new(base_port, llama_server_path);
        Self {
            core: Arc::new(RwLock::new(core)),
            strategy: ProcessStrategy::SingleSwap {
                catalog,
                current: Arc::new(RwLock::new(None)),
                loading: Arc::new(std::sync::RwLock::new(None)),
            },
        }
    }

    /// Start a llama-server instance for a model (Concurrent strategy only)
    pub async fn start_server(&self, config: ServerConfig) -> Result<u16> {
        let max_concurrent = match &self.strategy {
            ProcessStrategy::Concurrent { max_concurrent } => *max_concurrent,
            ProcessStrategy::SingleSwap { .. } => {
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
        // 1. Extract refs from strategy
        let (catalog, current_lock, loading_slot) = match &self.strategy {
            ProcessStrategy::SingleSwap {
                catalog,
                current,
                loading,
            } => (catalog, current, loading),
            ProcessStrategy::Concurrent { .. } => {
                return Err(ModelRuntimeError::Internal(
                    "ensure_model_running() is only available for SingleSwap strategy".to_string(),
                ));
            }
        };

        // 2. Retry loop with overall deadline (prevents unbounded waits through other models' swaps)
        let deadline = tokio::time::Instant::now() + STARTUP_WAIT_TIMEOUT;

        loop {
            let disposition = StartupDisposition::check(loading_slot, model_name.to_string());

            match disposition {
                StartupDisposition::Waiter {
                    rx,
                    target_model_name,
                } => {
                    // Check if this startup is for our model
                    if target_model_name == model_name {
                        // Yes — wait for the result (offset by 5s so driver always broadcasts first)
                        return wait_for_startup(rx, STARTUP_WAIT_TIMEOUT + Duration::from_secs(5))
                            .await;
                    }
                    // No — another model is starting. Wait for it to finish, then retry.
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    if should_bail_on_insufficient_budget(remaining) {
                        return Err(ModelRuntimeError::ContentionTimeout(
                            "Insufficient time remaining for model startup after contention"
                                .to_string(),
                        ));
                    }
                    let _ = wait_for_startup(rx, remaining).await;
                    // Loop back and re-check the slot
                }
                StartupDisposition::Initiator { guard, self_rx } => {
                    // 3. Clone everything needed for the 'static async block.
                    let core = self.core.clone();
                    let catalog_owned = catalog.clone();
                    let current_owned = current_lock.clone(); // Arc clone — cheap
                    let model_name_owned = model_name.to_string();

                    // 4. Spawn the driver task (detached from this request's future)
                    drive(guard, STARTUP_WAIT_TIMEOUT, async move {
                        // --- Model resolution ---
                        let launch_spec = catalog_owned
                            .resolve_for_launch(&model_name_owned)
                            .await
                            .map_err(|e| match e {
                                CatalogError::QueryFailed(msg) => ModelRuntimeError::Internal(msg),
                                CatalogError::Internal(msg) => ModelRuntimeError::Internal(msg),
                            })?
                            .ok_or_else(|| {
                                ModelRuntimeError::ModelNotFound(model_name_owned.clone())
                            })?;

                        let effective_ctx = num_ctx.unwrap_or(default_ctx);
                        let model_path = &launch_spec.file_path;

                        // Check model file exists
                        if !tokio::fs::try_exists(model_path).await.unwrap_or(false) {
                            return Err(ModelRuntimeError::ModelFileNotFound(
                                model_path.display().to_string(),
                            ));
                        }

                        // --- Cached instance check (fast path: already running + healthy) ---
                        let cached = {
                            let current_guard = current_owned.read().await;
                            current_guard.as_ref().and_then(|current| {
                                (current.model_id == launch_spec.id
                                    && current.context_size == effective_ctx)
                                    .then(|| {
                                        (
                                            current.port,
                                            current.model_id,
                                            current.model_name.clone(),
                                            current.context_size,
                                        )
                                    })
                            })
                        };
                        if let Some((port, model_id, cached_name, context_size)) = cached {
                            if check_http_health(port).await {
                                info!(
                                    model_id = %model_id,
                                    model_name = %cached_name,
                                    port = %port,
                                    context = %context_size,
                                    "Model already running with correct context"
                                );
                                return Ok(RunningTarget::local(
                                    port,
                                    model_id,
                                    cached_name,
                                    context_size,
                                ));
                            }
                            warn!(
                                model_id = %model_id,
                                port = %port,
                                "cached model failed health check; recycling degraded instance"
                            );
                        }

                        // --- Stop current model if running ---
                        {
                            let mut current_guard = current_owned.write().await;
                            if let Some(current) = current_guard.take() {
                                info!(
                                    model_id = %current.model_id,
                                    model_name = %current.model_name,
                                    "Stopping current model for swap"
                                );
                                let mut core_w = core.write().await;
                                if let Err(e) = core_w.kill(current.model_id).await {
                                    warn!(error = %e, "Failed to stop current model cleanly, continuing");
                                }
                            }
                        }

                        // Clean up any dead processes
                        {
                            let mut core_w = core.write().await;
                            core_w.cleanup_dead().await;
                        }

                        // --- Spawn new instance ---
                        info!(
                            model_id = %launch_spec.id,
                            model_name = %launch_spec.name,
                            context = %effective_ctx,
                            "Starting model"
                        );

                        let config = build_server_config(
                            launch_spec.id as i64,
                            launch_spec.name.clone(),
                            model_path.to_path_buf(),
                            0, // base_port unused — GuiProcessCore resolves port internally
                            &launch_spec.tags,
                            ServerConfigOptions {
                                context_size: num_ctx,
                                model_server_ctx: launch_spec
                                    .server_defaults
                                    .as_ref()
                                    .and_then(|sc| sc.context_length),
                                global_default_ctx: Some(default_ctx),
                                ..Default::default()
                            },
                        );

                        let port = {
                            let mut core_w = core.write().await;
                            core_w
                                .spawn(config)
                                .await
                                .map_err(|e| ModelRuntimeError::SpawnFailed(e.to_string()))?
                        };

                        // --- Wait for health check ---
                        if let Err(e) = wait_for_http_health(port, 120).await {
                            return Err(ModelRuntimeError::HealthCheckFailed(e.to_string()));
                        }

                        // --- SUCCESS: update current model state ---
                        {
                            let mut current_guard = current_owned.write().await;
                            *current_guard = Some(CurrentModelState {
                                model_id: launch_spec.id,
                                model_name: launch_spec.name.clone(),
                                context_size: effective_ctx,
                                port,
                                model_path: launch_spec.file_path.clone(),
                            });
                        }

                        info!(
                            model_id = %launch_spec.id,
                            model_name = %launch_spec.name,
                            port = %port,
                            context = %effective_ctx,
                            "Model started successfully"
                        );

                        Ok(RunningTarget::local(
                            port,
                            launch_spec.id,
                            launch_spec.name,
                            effective_ctx,
                        ))
                    });

                    // 5. Wait for result — same path as every other caller (offset by 5s so driver always broadcasts first)
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
            ProcessStrategy::SingleSwap { current, .. } => {
                let current = current.clone();
                let guard = current.read().await;
                guard.as_ref().map(|c| {
                    RunningTarget::local(c.port, c.model_id, c.model_name.clone(), c.context_size)
                })
            }
            ProcessStrategy::Concurrent { .. } => None,
        }
    }

    /// Stop the currently running model (SingleSwap only).
    pub async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        match &self.strategy {
            ProcessStrategy::SingleSwap { current, .. } => {
                let current = current.clone();
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

    /// Graceful shutdown
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down process manager");
        // For SingleSwap, also clear current model state
        if let ProcessStrategy::SingleSwap { current, .. } = &self.strategy {
            let current = current.clone();
            let mut guard = current.write().await;
            *guard = None;
        }
        self.stop_all().await
    }

    /// Check if this manager uses SingleSwap strategy.
    #[must_use]
    pub fn is_single_swap(&self) -> bool {
        matches!(self.strategy, ProcessStrategy::SingleSwap { .. })
    }

    /// Check if a model is currently loading (SingleSwap only).
    #[must_use]
    pub fn is_loading(&self) -> bool {
        match &self.strategy {
            ProcessStrategy::SingleSwap { loading, .. } => {
                loading.read().ok().map(|s| s.is_some()).unwrap_or(false)
            }
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
