//! Server service for managing model server processes.
//!
//! This service wraps a `ProcessRunner` to provide a clean API for
//! starting, stopping, and monitoring llama-server processes.

use std::sync::Arc;

use crate::domain::Model;
use crate::ports::{
    CoreError, ModelRepository, ProcessHandle, ProcessRunner, ServerConfig, ServerHealth,
};

/// Configuration for starting a server.
#[derive(Debug, Clone)]
pub struct StartConfig {
    /// Model ID to serve.
    pub model_id: i64,
    /// Optional context length override.
    pub context_size: Option<u64>,
    /// Optional port override (None = auto-allocate).
    pub port: Option<u16>,
    /// Optional GPU layers override.
    pub gpu_layers: Option<i32>,
    /// Extra arguments to pass to the server.
    pub extra_args: Vec<String>,
}

impl StartConfig {
    /// Create a minimal start configuration.
    pub fn new(model_id: i64) -> Self {
        Self {
            model_id,
            context_size: None,
            port: None,
            gpu_layers: None,
            extra_args: Vec::new(),
        }
    }

    /// Set context size.
    pub fn with_context_size(mut self, size: u64) -> Self {
        self.context_size = Some(size);
        self
    }

    /// Set port.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }
}

/// Service for managing background model server instances.
pub struct ServerService {
    runner: Arc<dyn ProcessRunner>,
    model_repo: Arc<dyn ModelRepository>,
}

impl ServerService {
    /// Create a new ServerService.
    pub fn new(runner: Arc<dyn ProcessRunner>, model_repo: Arc<dyn ModelRepository>) -> Self {
        Self { runner, model_repo }
    }

    /// Start a server for the given model.
    pub async fn start(&self, config: StartConfig) -> Result<ProcessHandle, CoreError> {
        // Look up the model
        let model = self
            .model_repo
            .get_by_id(config.model_id)
            .await
            .map_err(CoreError::Repository)?;

        // Build server config
        let server_config = self.build_server_config(&model, &config);

        // Start the server
        self.runner
            .start(server_config)
            .await
            .map_err(CoreError::Process)
    }

    /// Stop a running server.
    pub async fn stop(&self, handle: &ProcessHandle) -> Result<(), CoreError> {
        self.runner.stop(handle).await.map_err(CoreError::Process)
    }

    /// Check if a server is running.
    pub async fn is_running(&self, handle: &ProcessHandle) -> bool {
        self.runner.is_running(handle).await
    }

    /// Get health status of a server.
    pub async fn health(&self, handle: &ProcessHandle) -> Result<ServerHealth, CoreError> {
        self.runner.health(handle).await.map_err(CoreError::Process)
    }

    /// List all running servers.
    pub async fn list_running(&self) -> Result<Vec<ProcessHandle>, CoreError> {
        self.runner
            .list_running()
            .await
            .map_err(CoreError::Process)
    }

    /// Build a ServerConfig from model and start config.
    fn build_server_config(&self, model: &Model, config: &StartConfig) -> ServerConfig {
        let mut server_config =
            ServerConfig::new(model.id, model.name.clone(), model.file_path.clone());

        if let Some(ctx) = config.context_size {
            server_config = server_config.with_context_size(ctx);
        }

        if let Some(port) = config.port {
            server_config = server_config.with_port(port);
        }

        if let Some(layers) = config.gpu_layers {
            server_config = server_config.with_gpu_layers(layers);
        }

        if !config.extra_args.is_empty() {
            server_config = server_config.with_extra_args(config.extra_args.clone());
        }

        server_config
    }
}
