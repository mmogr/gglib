#![allow(clippy::collapsible_if)]

//! Unified process manager for llama-server instances.
//!
//! This module provides a single, flexible process manager that supports two strategies:
//! - **Concurrent**: Multiple models running simultaneously (GUI use case)
//! - **SingleSwap**: Auto-swapping single model with smart context handling (Proxy use case)
//!
//! This replaces the old MultiManager and ModelManager with a cleaner, unified API.

use crate::commands::common::resolve_jinja_flag;
use crate::models::Gguf;
use crate::services::database;
use crate::utils::process::{ProcessCore, ServerInfo, check_process_health, wait_for_http_health};
use anyhow::{Result, anyhow};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Strategy for managing llama-server processes
#[derive(Clone)]
pub enum ProcessStrategy {
    /// Allow multiple concurrent models up to max_concurrent
    Concurrent { max_concurrent: usize },

    /// Only allow one model at a time, auto-swap when different model requested
    SingleSwap { db_pool: SqlitePool },
}

/// Unified process manager for llama-server instances
///
/// Supports two strategies:
/// - Concurrent: Multiple models (GUI)
/// - SingleSwap: Auto-swapping single model (Proxy)
pub struct ProcessManager {
    core: Arc<RwLock<ProcessCore>>,
    strategy: ProcessStrategy,
    /// Tracks current model ID for SingleSwap strategy
    current_model: Arc<RwLock<Option<u32>>>,
}

impl ProcessManager {
    /// Create a new ProcessManager with Concurrent strategy (for GUI)
    pub fn new_concurrent(
        base_port: u16,
        max_concurrent: usize,
        llama_server_path: impl Into<String>,
    ) -> Self {
        let core = ProcessCore::new(base_port, llama_server_path);
        Self {
            core: Arc::new(RwLock::new(core)),
            strategy: ProcessStrategy::Concurrent { max_concurrent },
            current_model: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new ProcessManager with SingleSwap strategy (for Proxy)
    pub fn new_single_swap(
        db_pool: SqlitePool,
        base_port: u16,
        llama_server_path: impl Into<String>,
    ) -> Self {
        let core = ProcessCore::new(base_port, llama_server_path);
        Self {
            core: Arc::new(RwLock::new(core)),
            strategy: ProcessStrategy::SingleSwap { db_pool },
            current_model: Arc::new(RwLock::new(None)),
        }
    }

    /// Start a llama-server instance for a model
    ///
    /// Behavior depends on strategy:
    /// - Concurrent: Errors if already running or at capacity
    /// - SingleSwap: Auto-swaps if different model, compares context
    pub async fn start_server(
        &self,
        model_id: u32,
        model_name: String,
        model_path: &str,
        context_length: Option<u64>,
        jinja: bool,
    ) -> Result<u16> {
        match &self.strategy {
            ProcessStrategy::Concurrent { max_concurrent } => {
                self.start_server_concurrent(
                    model_id,
                    model_name,
                    model_path,
                    context_length,
                    *max_concurrent,
                    jinja,
                )
                .await
            }
            ProcessStrategy::SingleSwap { .. } => {
                // SingleSwap uses ensure_model_running instead
                Err(anyhow!(
                    "SingleSwap strategy should use ensure_model_running() instead of start_server()"
                ))
            }
        }
    }

    /// Start server with Concurrent strategy (GUI behavior)
    async fn start_server_concurrent(
        &self,
        model_id: u32,
        model_name: String,
        model_path: &str,
        context_length: Option<u64>,
        max_concurrent: usize,
        jinja: bool,
    ) -> Result<u16> {
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
        let path = std::path::Path::new(model_path);
        let port = core.spawn(model_id, model_name, path, context_length, jinja)?;

        // Release the lock before waiting
        drop(core);

        // Wait for server to be ready by polling health endpoint
        debug!(
            port = %port,
            "Waiting for llama-server to be ready"
        );
        self.wait_for_health(port).await?;
        debug!("llama-server is ready and accepting requests");

        Ok(port)
    }

    /// Ensure a model is running with SingleSwap strategy (Proxy behavior)
    ///
    /// This method:
    /// - Checks if the same model is already running with correct context
    /// - Restarts if context size changes
    /// - Swaps to different model if needed
    /// - Waits for HTTP health check before returning
    pub async fn ensure_model_running(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_context: u64,
    ) -> Result<u16> {
        // Only works with SingleSwap strategy
        let db_pool = match &self.strategy {
            ProcessStrategy::SingleSwap { db_pool } => db_pool.clone(),
            ProcessStrategy::Concurrent { .. } => {
                return Err(anyhow!(
                    "ensure_model_running() is only available for SingleSwap strategy"
                ));
            }
        };

        // Check if the same model is already running with correct context
        {
            let current_id = self.current_model.read().await;
            if let Some(id) = *current_id {
                let core = self.core.read().await;
                if let Some(info) = core.get_info(id) {
                    if info.model_name == model_name {
                        // Check if context size matches
                        let requested_ctx = num_ctx.unwrap_or(default_context);
                        if info.context_size == Some(requested_ctx) || num_ctx.is_none() {
                            info!(
                                "Model '{}' is already running on port {} with context size {}",
                                model_name,
                                info.port,
                                info.context_size
                                    .map(|c| c.to_string())
                                    .unwrap_or_else(|| "default".to_string())
                            );
                            return Ok(info.port);
                        }
                        info!(
                            "Context size change requested: {:?} -> {}",
                            info.context_size, requested_ctx
                        );
                        // Fall through to restart with new context
                    }
                }
            }
        }

        // Find the model to get its metadata
        let model = self.find_model(&db_pool, model_name).await?;
        let model_id = model.id.ok_or_else(|| anyhow!("Model has no ID"))?;

        // Determine context size
        let requested_ctx = num_ctx.unwrap_or(default_context);

        info!(
            "Starting model '{}' with context size {}",
            model_name, requested_ctx
        );

        // Stop current model only if it's different from the one we want
        let should_stop = {
            let current_id = self.current_model.read().await;
            if let Some(id) = *current_id {
                id != model_id
            } else {
                false
            }
        }; // Read lock released here

        if should_stop {
            self.stop_current_model().await?;
        }

        // Start the new model
        let jinja_resolution = resolve_jinja_flag(None, &model.tags);

        let port = self
            .start_model_single_swap(
                &model,
                model_id,
                Some(requested_ctx),
                jinja_resolution.enabled,
            )
            .await?;

        Ok(port)
    }

    /// Find a model by name in the database (SingleSwap helper)
    async fn find_model(&self, db_pool: &SqlitePool, model_name: &str) -> Result<Gguf> {
        // Try exact name match first
        if let Ok(Some(model)) = database::find_model_by_identifier(db_pool, model_name).await {
            return Ok(model);
        }

        // Try case-insensitive search
        let query = "SELECT * FROM models WHERE LOWER(name) = LOWER(?) LIMIT 1";
        let model: Option<Gguf> = sqlx::query_as::<_, Gguf>(query)
            .bind(model_name)
            .fetch_optional(db_pool)
            .await?;

        model.ok_or_else(|| anyhow!("Model '{}' not found in database", model_name))
    }

    /// Start a llama-server instance for SingleSwap strategy
    async fn start_model_single_swap(
        &self,
        model: &Gguf,
        model_id: u32,
        context_size: Option<u64>,
        jinja: bool,
    ) -> Result<u16> {
        info!(
            "Starting llama-server for model '{}' on port allocation",
            model.name
        );

        // Check if model file exists
        if !model.file_path.exists() {
            return Err(anyhow!(
                "Model file not found: {}",
                model.file_path.display()
            ));
        }

        // Spawn the process
        let port = {
            let mut core = self.core.write().await;
            core.spawn(
                model_id,
                model.name.clone(),
                &model.file_path,
                context_size,
                jinja,
            )?
        };

        // Wait for the server to be ready (2 minutes timeout for large models)
        wait_for_http_health(port, 120).await?;

        // Update current model tracking
        {
            let mut current_id = self.current_model.write().await;
            *current_id = Some(model_id);
        }

        Ok(port)
    }

    /// Stop the currently running model (SingleSwap)
    async fn stop_current_model(&self) -> Result<()> {
        let id = {
            let current_id = self.current_model.read().await;
            *current_id
        };

        if let Some(id) = id {
            let model_name = {
                let core = self.core.read().await;
                core.get_info(id).map(|info| info.model_name.clone())
            };

            if let Some(name) = model_name {
                info!("Stopping model '{}'", name);
            }

            // Try to kill the process (non-blocking)
            let kill_result = {
                let mut core = self.core.write().await;
                core.kill(id)
            };

            // Clear current model tracking immediately
            {
                let mut current_id_mut = self.current_model.write().await;
                *current_id_mut = None;
            }

            // Log error but don't fail
            if let Err(e) = kill_result {
                tracing::warn!("Failed to stop model cleanly: {}. Continuing anyway.", e);
            }

            // Brief delay to let port be released
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }

        Ok(())
    }

    /// Stop a specific server by model ID
    pub async fn stop_server(&self, model_id: u32) -> Result<()> {
        let mut core = self.core.write().await;
        core.kill(model_id)
    }

    /// Get information about a specific running server
    pub async fn get_server(&self, model_id: u32) -> Option<ServerInfo> {
        let core = self.core.read().await;
        core.get_info(model_id).cloned()
    }

    /// Get information about all running servers
    pub async fn list_servers(&self) -> Vec<ServerInfo> {
        let core = self.core.read().await;
        core.list_all().into_iter().cloned().collect()
    }

    /// Check if a model is currently being served
    pub async fn is_serving(&self, model_id: u32) -> bool {
        let core = self.core.read().await;
        core.is_running(model_id)
    }

    /// Update health status of all servers (Concurrent strategy)
    ///
    /// This checks if processes are still alive and updates health flags.
    /// Also removes dead processes from tracking.
    pub async fn update_health_status(&self) {
        let mut core = self.core.write().await;

        // Clean up dead processes first
        let _ = core.cleanup_dead();

        // Update health status for remaining processes
        let servers: Vec<(u32, u32)> = core
            .list_all()
            .iter()
            .map(|info| (info.model_id, info.pid))
            .collect();

        for (model_id, pid) in servers {
            if let Some(info) = core.get_info_mut(model_id) {
                info.healthy = check_process_health(pid);
            }
        }
    }

    /// Stop all running servers
    pub async fn stop_all(&self) -> Result<()> {
        let mut core = self.core.write().await;
        core.kill_all();
        Ok(())
    }

    /// Get the port for the currently running model (SingleSwap)
    pub async fn get_current_port(&self) -> Option<u16> {
        let current_id = self.current_model.read().await;
        if let Some(id) = *current_id {
            let core = self.core.read().await;
            core.get_info(id).map(|info| info.port)
        } else {
            None
        }
    }

    /// Get the currently running model name (SingleSwap)
    pub async fn get_current_model(&self) -> Option<String> {
        let current_id = self.current_model.read().await;
        if let Some(id) = *current_id {
            let core = self.core.read().await;
            core.get_info(id).map(|info| info.model_name.clone())
        } else {
            None
        }
    }

    /// Wait for llama-server to be ready by polling /health endpoint
    async fn wait_for_health(&self, port: u16) -> Result<()> {
        let url = format!("http://127.0.0.1:{}/health", port);
        let client = reqwest::Client::new();
        let max_attempts = 120; // 2 minutes max (120 * 1 second)

        for attempt in 1..=max_attempts {
            match client
                .get(&url)
                .timeout(std::time::Duration::from_secs(1))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    debug!(attempt = %attempt, "Health check passed");
                    return Ok(());
                }
                Ok(response) if response.status() == 503 => {
                    // Server is loading model, this is expected
                    if attempt % 5 == 0 {
                        debug!(attempt = %attempt, "Model still loading");
                    }
                }
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await;
                    warn!(
                        status = %status,
                        body = ?body,
                        "Unexpected health check status"
                    );
                }
                Err(e) if e.is_connect() || e.is_timeout() => {
                    // Connection refused or timeout - server not ready yet
                    if attempt % 10 == 0 {
                        debug!(attempt = %attempt, "Waiting for server to start");
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Health check error");
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        Err(anyhow!(
            "Server failed to become healthy after {} seconds",
            max_attempts
        ))
    }

    /// Get the database pool (for SingleSwap strategy)
    pub fn get_db_pool(&self) -> Option<SqlitePool> {
        match &self.strategy {
            ProcessStrategy::SingleSwap { db_pool } => Some(db_pool.clone()),
            ProcessStrategy::Concurrent { .. } => None,
        }
    }

    /// Graceful shutdown for SingleSwap strategy
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down process manager");
        self.stop_current_model().await
    }
}

impl Clone for ProcessManager {
    fn clone(&self) -> Self {
        Self {
            core: self.core.clone(),
            strategy: self.strategy.clone(),
            current_model: self.current_model.clone(),
        }
    }
}

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
}
