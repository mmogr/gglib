//! `ProcessRunner` implementation for llama-server.
//!
//! This module provides the `LlamaServerRunner` which implements the
//! `ProcessRunner` trait from `gglib-core`, managing llama-server processes.

use async_trait::async_trait;
use gglib_core::ports::{ProcessError, ProcessHandle, ProcessRunner, ServerConfig, ServerHealth};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

use crate::health::{check_http_health, wait_for_http_health};
use crate::process_core::ProcessCore;

/// Default timeout for health checks when starting a server (seconds).
const DEFAULT_STARTUP_TIMEOUT_SECS: u64 = 120;

/// `ProcessRunner` implementation using llama-server.
///
/// This runner spawns and manages local llama-server processes.
/// It implements the `ProcessRunner` trait for use with `AppCore`.
///
/// # Design
///
/// - Pure OS/process concerns only
/// - No database or domain logic
/// - Accepts `ServerConfig` and executes it
pub struct LlamaServerRunner {
    core: Arc<RwLock<ProcessCore>>,
    /// Maximum concurrent servers (0 = unlimited).
    max_concurrent: usize,
}

impl LlamaServerRunner {
    /// Create a new `LlamaServerRunner`.
    ///
    /// # Arguments
    ///
    /// * `llama_server_path` - Path to the llama-server binary
    /// * `max_concurrent` - Maximum concurrent servers (0 = unlimited)
    pub fn new(llama_server_path: impl Into<PathBuf>, max_concurrent: usize) -> Self {
        let core = ProcessCore::new(llama_server_path);
        Self {
            core: Arc::new(RwLock::new(core)),
            max_concurrent,
        }
    }

    /// Create a runner with no concurrency limit.
    pub fn unlimited(llama_server_path: impl Into<PathBuf>) -> Self {
        Self::new(llama_server_path, 0)
    }

    /// Create a runner for single-server mode (max 1 concurrent).
    pub fn single(llama_server_path: impl Into<PathBuf>) -> Self {
        Self::new(llama_server_path, 1)
    }

    /// Get the path to the llama-server binary.
    pub async fn llama_server_path(&self) -> String {
        let core = self.core.read().await;
        // Access through ProcessCore if needed, or store separately
        // For now, we don't expose this from ProcessCore
        drop(core);
        String::new() // TODO: Store path in runner if needed
    }
}

#[async_trait]
impl ProcessRunner for LlamaServerRunner {
    async fn start(&self, config: ServerConfig) -> Result<ProcessHandle, ProcessError> {
        debug!(
            model_id = %config.model_id,
            model_name = %config.model_name,
            "Starting server"
        );

        // Check concurrency limit
        if self.max_concurrent > 0 {
            let core = self.core.read().await;
            if core.count() >= self.max_concurrent {
                return Err(ProcessError::ResourceExhausted(format!(
                    "Maximum concurrent servers ({}) reached",
                    self.max_concurrent
                )));
            }
        }

        // Spawn the process
        let handle = {
            let mut core = self.core.write().await;
            core.spawn(&config)
                .await
                .map_err(|e| ProcessError::StartFailed(e.to_string()))?
        };

        // Wait for HTTP health check
        wait_for_http_health(handle.port, DEFAULT_STARTUP_TIMEOUT_SECS)
            .await
            .map_err(|e| {
                // Try to kill the process if health check fails
                let model_id = config.model_id;
                let core = self.core.clone();
                tokio::spawn(async move {
                    let mut core = core.write().await;
                    let _ = core.kill(model_id).await;
                });
                ProcessError::StartFailed(format!("Health check failed: {}", e))
            })?;

        debug!(
            model_id = %config.model_id,
            port = %handle.port,
            "Server started successfully"
        );

        Ok(handle)
    }

    async fn stop(&self, handle: &ProcessHandle) -> Result<(), ProcessError> {
        debug!(
            model_id = %handle.model_id,
            port = %handle.port,
            "Stopping server"
        );

        let mut core = self.core.write().await;

        if !core.is_running(handle.model_id) {
            return Err(ProcessError::NotRunning(format!(
                "Model {} is not running",
                handle.model_id
            )));
        }

        core.kill(handle.model_id)
            .await
            .map_err(|e| ProcessError::StopFailed(e.to_string()))
    }

    async fn is_running(&self, handle: &ProcessHandle) -> bool {
        let core = self.core.read().await;
        core.is_running(handle.model_id)
    }

    async fn health(&self, handle: &ProcessHandle) -> Result<ServerHealth, ProcessError> {
        let core = self.core.read().await;

        if !core.is_running(handle.model_id) {
            return Err(ProcessError::NotRunning(format!(
                "Model {} is not running",
                handle.model_id
            )));
        }

        let context_size = core.get_context_size(handle.model_id);
        drop(core);

        // Check HTTP health
        match check_http_health(handle.port).await {
            Ok(true) => {
                let mut health = ServerHealth::healthy();
                if let Some(ctx) = context_size {
                    health = health.with_context_size(ctx);
                }
                Ok(health)
            }
            Ok(false) => Ok(ServerHealth::unhealthy("Health check returned non-200")),
            Err(e) => Ok(ServerHealth::unhealthy(format!(
                "Health check error: {}",
                e
            ))),
        }
    }

    async fn list_running(&self) -> Result<Vec<ProcessHandle>, ProcessError> {
        let mut core = self.core.write().await;

        // Clean up dead processes first
        core.cleanup_dead().await;

        Ok(core.list_all())
    }
}
