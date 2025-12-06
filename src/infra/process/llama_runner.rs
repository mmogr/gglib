//! Llama-server implementation of the ProcessRunner trait.
//!
//! This implementation wraps the existing ProcessCore and ProcessManager
//! to provide the trait interface while preserving current behavior.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::core::ports::ProcessError;
use crate::core::ports::process_runner::{
    ProcessHandle, ProcessRunner, ServerConfig, ServerHealth,
};
use crate::utils::process::{ProcessCore, check_process_health, wait_for_http_health};

/// Strategy for managing llama-server processes.
#[derive(Clone, Copy, Debug)]
pub enum RunnerStrategy {
    /// Allow multiple concurrent models up to max_concurrent.
    Concurrent { max_concurrent: usize },
    /// Only allow one model at a time, auto-swap when different model requested.
    SingleSwap,
}

/// Llama-server implementation of the ProcessRunner trait.
///
/// This implementation manages llama-server processes for model inference.
/// It wraps the low-level `ProcessCore` and provides the trait interface.
pub struct LlamaProcessRunner {
    core: Arc<RwLock<ProcessCore>>,
    strategy: RunnerStrategy,
    /// Tracks currently running models by model_id -> ProcessHandle
    handles: Arc<RwLock<HashMap<i64, ProcessHandle>>>,
}

impl LlamaProcessRunner {
    /// Create a new LlamaProcessRunner with Concurrent strategy.
    ///
    /// This is suitable for GUI use cases where multiple models can run simultaneously.
    pub fn new_concurrent(
        base_port: u16,
        max_concurrent: usize,
        llama_server_path: impl Into<String>,
    ) -> Self {
        let core = ProcessCore::new(base_port, llama_server_path);
        Self {
            core: Arc::new(RwLock::new(core)),
            strategy: RunnerStrategy::Concurrent { max_concurrent },
            handles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new LlamaProcessRunner with SingleSwap strategy.
    ///
    /// This is suitable for proxy use cases where only one model runs at a time.
    pub fn new_single_swap(base_port: u16, llama_server_path: impl Into<String>) -> Self {
        let core = ProcessCore::new(base_port, llama_server_path);
        Self {
            core: Arc::new(RwLock::new(core)),
            strategy: RunnerStrategy::SingleSwap,
            handles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the current strategy.
    pub fn strategy(&self) -> RunnerStrategy {
        self.strategy
    }

    /// Wait for server to become healthy (HTTP health check).
    async fn wait_for_health(&self, port: u16) -> Result<(), ProcessError> {
        // Default timeout of 60 seconds for server startup
        wait_for_http_health(port, 60)
            .await
            .map_err(|e| ProcessError::HealthCheckFailed(e.to_string()))
    }
}

#[async_trait]
impl ProcessRunner for LlamaProcessRunner {
    async fn start(&self, config: ServerConfig) -> Result<ProcessHandle, ProcessError> {
        let mut core = self.core.write().await;
        let mut handles = self.handles.write().await;

        let model_id_u32 = config.model_id as u32;

        // Check strategy constraints
        match self.strategy {
            RunnerStrategy::Concurrent { max_concurrent } => {
                // Check if already running
                if core.is_running(model_id_u32) {
                    return Err(ProcessError::StartFailed(format!(
                        "Model {} is already being served",
                        config.model_name
                    )));
                }

                // Check concurrent limit
                if core.count() >= max_concurrent {
                    return Err(ProcessError::ResourceExhausted(format!(
                        "Maximum concurrent servers ({}) reached. Stop a server first.",
                        max_concurrent
                    )));
                }
            }
            RunnerStrategy::SingleSwap => {
                // For SingleSwap, stop any running model first
                if !handles.is_empty() {
                    // Get the current model id
                    if let Some((&current_id, _)) = handles.iter().next() {
                        core.kill(current_id as u32)
                            .map_err(|e| ProcessError::StopFailed(e.to_string()))?;
                        handles.remove(&current_id);
                    }
                }
            }
        }

        // Spawn the process
        let path = config.model_path.as_path();
        let port = core
            .spawn(
                model_id_u32,
                config.model_name.clone(),
                path,
                config.context_size,
                config.port,
                false, // jinja - could be added to ServerConfig if needed
                None,  // reasoning_format - could be added to ServerConfig if needed
            )
            .map_err(|e| ProcessError::StartFailed(e.to_string()))?;

        // Get the PID from core
        let pid = core.get_info(model_id_u32).map(|info| info.pid);

        let started_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let handle = ProcessHandle::new(config.model_id, config.model_name, pid, port, started_at);

        // Store the handle
        handles.insert(config.model_id, handle.clone());

        // Release locks before waiting for health
        drop(handles);
        drop(core);

        // Wait for server to be ready
        self.wait_for_health(port).await?;

        Ok(handle)
    }

    async fn stop(&self, handle: &ProcessHandle) -> Result<(), ProcessError> {
        let mut core = self.core.write().await;
        let mut handles = self.handles.write().await;

        let model_id_u32 = handle.model_id as u32;

        if !core.is_running(model_id_u32) {
            return Err(ProcessError::NotRunning(format!(
                "Model {} is not running",
                handle.model_name
            )));
        }

        core.kill(model_id_u32)
            .map_err(|e| ProcessError::StopFailed(e.to_string()))?;

        handles.remove(&handle.model_id);

        Ok(())
    }

    async fn is_running(&self, handle: &ProcessHandle) -> bool {
        let core = self.core.read().await;
        core.is_running(handle.model_id as u32)
    }

    async fn health(&self, handle: &ProcessHandle) -> Result<ServerHealth, ProcessError> {
        let core = self.core.read().await;

        if !core.is_running(handle.model_id as u32) {
            return Err(ProcessError::NotRunning(format!(
                "Model {} is not running",
                handle.model_name
            )));
        }

        // Check process health using PID if available
        let is_healthy = handle.pid.map(check_process_health).unwrap_or(false);

        let info = core.get_info(handle.model_id as u32);
        let context_size = info.and_then(|i| i.context_size);

        if is_healthy {
            Ok(ServerHealth::healthy().with_context_size(context_size.unwrap_or(0)))
        } else {
            Ok(ServerHealth::unhealthy("Health check failed"))
        }
    }

    async fn list_running(&self) -> Result<Vec<ProcessHandle>, ProcessError> {
        let handles = self.handles.read().await;
        Ok(handles.values().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_server_config_builder() {
        let config = ServerConfig::new(
            1,
            "test-model".to_string(),
            PathBuf::from("/path/to/model.gguf"),
        )
        .with_port(8080)
        .with_context_size(4096)
        .with_gpu_layers(32);

        assert_eq!(config.model_id, 1);
        assert_eq!(config.model_name, "test-model");
        assert_eq!(config.port, Some(8080));
        assert_eq!(config.context_size, Some(4096));
        assert_eq!(config.gpu_layers, Some(32));
    }

    #[test]
    fn test_process_handle_creation() {
        let handle = ProcessHandle::new(42, "my-model".to_string(), Some(12345), 8080, 1000);

        assert_eq!(handle.model_id, 42);
        assert_eq!(handle.model_name, "my-model");
        assert_eq!(handle.pid, Some(12345));
        assert_eq!(handle.port, 8080);
        assert_eq!(handle.started_at, 1000);
    }

    #[test]
    fn test_server_health_helpers() {
        let healthy = ServerHealth::healthy();
        assert!(healthy.healthy);
        assert!(healthy.last_check.is_some());

        let unhealthy = ServerHealth::unhealthy("test error");
        assert!(!unhealthy.healthy);
        assert_eq!(unhealthy.message, Some("test error".to_string()));

        let with_ctx = ServerHealth::healthy().with_context_size(4096);
        assert_eq!(with_ctx.context_size, Some(4096));
    }
}
