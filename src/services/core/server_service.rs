//! Server service for managing llama-server instances.
//!
//! This service wraps `ProcessManager` to provide a clean API for starting,
//! stopping, and monitoring llama-server processes. Used by the GUI for
//! background server management.
//!
//! Note: The CLI `serve` command runs llama-server as a blocking foreground
//! process with inherited stdio, which is a different use case and doesn't
//! use this service.

use crate::commands::llama_args::{
    JinjaResolutionSource, ReasoningFormatSource, resolve_jinja_flag, resolve_reasoning_format,
};
use crate::models::gui::StartServerResponse;
use crate::services::core::ModelService;
use crate::services::process_manager::ProcessManager;
use crate::utils::paths::get_llama_server_path;
use crate::utils::process::ServerInfo;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tracing::{debug, info};

/// Configuration for starting a server
#[derive(Debug, Clone)]
pub struct StartServerConfig {
    /// Model ID to serve
    pub model_id: u32,
    /// Optional context length override
    pub context_length: Option<u64>,
    /// Optional port override (None = auto-allocate)
    pub port: Option<u16>,
    /// Optional explicit jinja flag (None = auto-detect from tags)
    pub jinja: Option<bool>,
    /// Optional explicit reasoning format (None = auto-detect from tags)
    /// Valid values: "none", "deepseek", "deepseek-legacy"
    pub reasoning_format: Option<String>,
}

/// Service for managing background llama-server instances.
///
/// This service is designed for GUI use cases where multiple servers
/// may run concurrently in the background. For CLI foreground serving,
/// see `commands::serve`.
#[derive(Clone)]
pub struct ServerService {
    process_manager: Arc<ProcessManager>,
    model_service: ModelService,
}

impl ServerService {
    /// Create a new ServerService with the given ProcessManager and ModelService.
    ///
    /// # Arguments
    ///
    /// * `process_manager` - Shared ProcessManager instance
    /// * `model_service` - ModelService for looking up model details
    pub fn new(process_manager: Arc<ProcessManager>, model_service: ModelService) -> Self {
        Self {
            process_manager,
            model_service,
        }
    }

    /// Create a new ServerService with a new concurrent ProcessManager.
    ///
    /// This is a convenience method for creating a standalone service.
    ///
    /// # Arguments
    ///
    /// * `model_service` - ModelService for looking up model details
    /// * `base_port` - Base port for server instances
    /// * `max_concurrent` - Maximum number of concurrent servers
    pub fn new_concurrent(
        model_service: ModelService,
        base_port: u16,
        max_concurrent: usize,
    ) -> Result<Self> {
        let llama_server_path = get_llama_server_path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "llama-server".to_string());

        let process_manager = Arc::new(ProcessManager::new_concurrent(
            base_port,
            max_concurrent,
            llama_server_path,
        ));

        Ok(Self {
            process_manager,
            model_service,
        })
    }

    /// Get the underlying ProcessManager for advanced operations.
    pub fn process_manager(&self) -> Arc<ProcessManager> {
        self.process_manager.clone()
    }

    /// Start a llama-server instance for a model.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for the server to start
    ///
    /// # Returns
    ///
    /// Returns the port and a success message on success.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Model is not found in the database
    /// - Model file doesn't exist on disk
    /// - Server fails to start
    /// - Maximum concurrent servers reached (for concurrent strategy)
    pub async fn start(&self, config: StartServerConfig) -> Result<StartServerResponse> {
        let model_id = config.model_id;
        debug!(model_id = %model_id, "Starting server for model");

        // Get model from database via ModelService
        let model = self.model_service.get_by_id(model_id).await?;

        debug!(
            model_name = %model.name,
            model_path = %model.file_path.display(),
            "Found model"
        );

        // Check if model file exists
        if !model.file_path.exists() {
            return Err(anyhow!(
                "Model file not found: {}",
                model.file_path.display()
            ));
        }

        // Use context length from config, model metadata, or None
        let context_length = config.context_length.or(model.context_length);

        // Resolve jinja flag
        let jinja_resolution = resolve_jinja_flag(config.jinja, &model.tags);
        match (jinja_resolution.enabled, jinja_resolution.source) {
            (true, JinjaResolutionSource::ExplicitTrue) => {
                debug!("Enabling Jinja templates (user override)");
            }
            (true, JinjaResolutionSource::AgentTag) => {
                debug!("Enabling Jinja templates due to 'agent' tag");
            }
            (false, JinjaResolutionSource::ExplicitFalse) => {
                debug!("Jinja templates disabled by user override");
            }
            _ => {}
        }

        // Resolve reasoning format
        let reasoning_resolution = resolve_reasoning_format(config.reasoning_format, &model.tags);
        match (
            reasoning_resolution.format.as_ref(),
            reasoning_resolution.source,
        ) {
            (Some(format), ReasoningFormatSource::Explicit) => {
                debug!(format = %format, "Enabling reasoning format (user override)");
            }
            (Some(format), ReasoningFormatSource::ReasoningTag) => {
                debug!(format = %format, "Enabling reasoning format due to 'reasoning' tag");
            }
            _ => {}
        }

        debug!("Calling ProcessManager.start_server");

        // Start the server
        let port = self
            .process_manager
            .start_server(
                model_id,
                model.name.clone(),
                &model.file_path.to_string_lossy(),
                context_length,
                config.port,
                jinja_resolution.enabled,
                reasoning_resolution.format,
            )
            .await?;

        info!(port = %port, model_id = %model_id, "Server started");

        Ok(StartServerResponse {
            port,
            message: format!("Server started for model '{}' on port {}", model.name, port),
        })
    }

    /// Stop a running server for a model.
    ///
    /// # Arguments
    ///
    /// * `model_id` - ID of the model whose server to stop
    ///
    /// # Errors
    ///
    /// Returns an error if no server is running for this model.
    pub async fn stop(&self, model_id: u32) -> Result<String> {
        self.process_manager.stop_server(model_id).await?;
        Ok(format!("Model {} stopped successfully", model_id))
    }

    /// List all running servers.
    ///
    /// Updates health status before returning the list.
    pub async fn list(&self) -> Vec<ServerInfo> {
        // Update health before listing
        self.process_manager.update_health_status().await;
        let servers = self.process_manager.list_servers().await;

        debug!(server_count = %servers.len(), "list_servers called");
        for server in &servers {
            debug!(
                model_id = %server.model_id,
                model_name = %server.model_name,
                port = %server.port,
                "Server info"
            );
        }

        servers
    }

    /// Get server info for a specific model.
    ///
    /// # Arguments
    ///
    /// * `model_id` - ID of the model to check
    ///
    /// # Returns
    ///
    /// Returns `Some(ServerInfo)` if a server is running for this model,
    /// `None` otherwise.
    pub async fn get(&self, model_id: u32) -> Option<ServerInfo> {
        self.process_manager.get_server(model_id).await
    }

    /// Check if a server is running for a model.
    pub async fn is_running(&self, model_id: u32) -> bool {
        self.process_manager.get_server(model_id).await.is_some()
    }

    /// Update health status for all running servers.
    ///
    /// Checks each server's health endpoint and updates status accordingly.
    /// Called automatically by `list()`, but can be called explicitly before
    /// `get()` if you need fresh status.
    pub async fn update_health(&self) {
        self.process_manager.update_health_status().await;
    }

    /// Shutdown all running servers.
    pub async fn shutdown_all(&self) -> Result<()> {
        self.process_manager.shutdown().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database;

    #[tokio::test]
    async fn test_server_service_creation() {
        let pool = database::setup_test_database().await.unwrap();
        let model_service = ModelService::new(pool);

        let result = ServerService::new_concurrent(model_service, 9000, 5);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_server_service_list_empty() {
        let pool = database::setup_test_database().await.unwrap();
        let model_service = ModelService::new(pool);
        let service = ServerService::new_concurrent(model_service, 9000, 5).unwrap();

        let servers = service.list().await;
        // Should return empty list when no servers running
        assert!(servers.is_empty());
    }
}
