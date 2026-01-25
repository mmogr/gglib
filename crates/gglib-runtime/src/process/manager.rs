//! Unified process manager for llama-server instances.
//!
//! This module provides a high-level process manager that supports two strategies:
//! - **Concurrent**: Multiple models running simultaneously (GUI use case)
//! - **SingleSwap**: Auto-swapping single model with smart context handling (Proxy use case)

use super::core::GuiProcessCore;
use super::health::wait_for_http_health;
use super::types::{ServerInfo, SpawnConfig};
use anyhow::{Result, anyhow};
use gglib_core::ports::{CatalogError, ModelCatalogPort, ModelRuntimeError, RunningTarget};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

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
    SingleSwap {
        /// Model catalog for resolving model names and getting launch specs.
        catalog: Arc<dyn ModelCatalogPort>,
        /// Currently running model state.
        current: RwLock<Option<CurrentModelState>>,
        /// True if a model is currently being loaded (prevents thrashing).
        loading: AtomicBool,
    },
}

/// Scope guard that clears the loading flag on drop.
///
/// This ensures the loading flag is always cleared, even on error paths.
struct LoadingGuard<'a> {
    loading: &'a AtomicBool,
}

impl<'a> LoadingGuard<'a> {
    fn new(loading: &'a AtomicBool) -> Self {
        loading.store(true, Ordering::SeqCst);
        Self { loading }
    }
}

impl Drop for LoadingGuard<'_> {
    fn drop(&mut self) {
        self.loading.store(false, Ordering::SeqCst);
    }
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

    /// Create a new `ProcessManager` with SingleSwap strategy (for Proxy)
    ///
    /// # Arguments
    ///
    /// * `base_port` - Base port for llama-server allocation
    /// * `llama_server_path` - Path to llama-server binary
    /// * `catalog` - Model catalog for resolving model names and getting launch specs
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
                current: RwLock::new(None),
                loading: AtomicBool::new(false),
            },
        }
    }

    /// Start a llama-server instance for a model (Concurrent strategy only)
    pub async fn start_server(&self, config: SpawnConfig) -> Result<u16> {
        let max_concurrent = match &self.strategy {
            ProcessStrategy::Concurrent { max_concurrent } => *max_concurrent,
            ProcessStrategy::SingleSwap { .. } => {
                return Err(anyhow!(
                    "SingleSwap strategy should use ensure_model_running() instead of start_server()"
                ));
            }
        };

        let mut core = self.core.write().await;

        // Check if already running
        if core.is_running(config.model_id) {
            return Err(anyhow!("Model {} is already being served", config.model_id));
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
    /// 1. Resolves the model name to a database entry (by model_id)
    /// 2. Checks if the same model_id is already running with correct context
    /// 3. Restarts if context size changes (even for same model)
    /// 4. Swaps to different model if needed
    /// 5. Returns target information for routing
    ///
    /// # Errors
    ///
    /// Returns `ModelRuntimeError` if the model cannot be started.
    pub async fn ensure_model_running(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        let (catalog, current_lock, loading) = match &self.strategy {
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

        // 1. If already loading, return error immediately (prevents thrashing)
        if loading.load(Ordering::SeqCst) {
            return Err(ModelRuntimeError::ModelLoading);
        }

        // 2. RESOLVE MODEL FOR LAUNCH to get model_id AND file_path
        let launch_spec = catalog
            .resolve_for_launch(model_name)
            .await
            .map_err(|e| match e {
                CatalogError::QueryFailed(msg) => ModelRuntimeError::Internal(msg),
                CatalogError::Internal(msg) => ModelRuntimeError::Internal(msg),
            })?
            .ok_or_else(|| ModelRuntimeError::ModelNotFound(model_name.to_string()))?;

        let effective_ctx = num_ctx.unwrap_or(default_ctx);
        let model_path = &launch_spec.file_path;

        // Check model file exists
        if !model_path.exists() {
            return Err(ModelRuntimeError::ModelFileNotFound(
                model_path.display().to_string(),
            ));
        }

        // 3. Compare by model_id (not name) + context for "already running"
        {
            let current_guard = current_lock.read().await;
            if let Some(current) = current_guard.as_ref()
                && current.model_id == launch_spec.id
                && current.context_size == effective_ctx
            {
                // Same model, same context -> return existing
                info!(
                    model_id = %launch_spec.id,
                    model_name = %launch_spec.name,
                    port = %current.port,
                    context = %current.context_size,
                    "Model already running with correct context"
                );
                return Ok(RunningTarget::local(
                    current.port,
                    current.model_id,
                    current.model_name.clone(),
                    current.context_size,
                ));
            }
        }

        // 4. Need restart: different model_id OR context mismatch
        // Use LoadingGuard to ensure flag is cleared on any exit path
        let _guard = LoadingGuard::new(loading);

        // Stop current model if running
        {
            let mut current_guard = current_lock.write().await;
            if let Some(current) = current_guard.take() {
                info!(
                    model_id = %current.model_id,
                    model_name = %current.model_name,
                    "Stopping current model for swap"
                );
                let mut core = self.core.write().await;
                if let Err(e) = core.kill(current.model_id).await {
                    warn!(error = %e, "Failed to stop current model cleanly, continuing");
                }
            }
        }

        // Clean up any dead processes
        {
            let mut core = self.core.write().await;
            core.cleanup_dead().await;
        }

        // 5. Spawn new instance
        info!(
            model_id = %launch_spec.id,
            model_name = %launch_spec.name,
            context = %effective_ctx,
            "Starting model"
        );

        let config = SpawnConfig::new(
            launch_spec.id,
            launch_spec.name.clone(),
            model_path.to_path_buf(),
        )
        .with_context_size(effective_ctx)
        .with_jinja(); // Enable jinja by default for proxy

        let port = {
            let mut core = self.core.write().await;
            core.spawn(config)
                .await
                .map_err(|e| ModelRuntimeError::SpawnFailed(e.to_string()))?
        };

        // 6. Wait for health check
        if let Err(e) = wait_for_http_health(port, 120).await {
            // DON'T update current_model on failure - guard will clear loading
            return Err(ModelRuntimeError::HealthCheckFailed(e.to_string()));
        }

        // 7. SUCCESS: update current model state
        {
            let mut current_guard = current_lock.write().await;
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

        // Guard will be dropped here, clearing loading flag
        Ok(RunningTarget::local(
            port,
            launch_spec.id,
            launch_spec.name,
            effective_ctx,
        ))
    }

    /// Get information about the currently running model (SingleSwap only).
    pub async fn current_model(&self) -> Option<RunningTarget> {
        match &self.strategy {
            ProcessStrategy::SingleSwap { current, .. } => {
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
            ProcessStrategy::SingleSwap { loading, .. } => loading.load(Ordering::SeqCst),
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
