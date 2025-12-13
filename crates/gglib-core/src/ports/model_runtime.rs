//! Model runtime port for proxy model management.
//!
//! This port defines the interface for ensuring a model is running
//! and ready to serve requests. It abstracts the process management
//! details from the proxy layer.

use async_trait::async_trait;
use std::fmt;
use thiserror::Error;

/// Target information for a running model instance.
///
/// This struct contains all information needed to route requests
/// to a running llama-server instance.
#[derive(Debug, Clone)]
pub struct RunningTarget {
    /// Full URL to the server (e.g., <http://127.0.0.1:5500>).
    /// Future-proof for non-localhost deployments.
    pub base_url: String,
    /// Port the server is listening on.
    pub port: u16,
    /// Database ID of the model.
    pub model_id: u32,
    /// Human-readable model name (for logging/headers).
    pub model_name: String,
    /// Actual context size being used.
    pub effective_ctx: u64,
}

impl RunningTarget {
    /// Create a new `RunningTarget` for a local server.
    #[must_use]
    pub fn local(port: u16, model_id: u32, model_name: String, effective_ctx: u64) -> Self {
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            port,
            model_id,
            model_name,
            effective_ctx,
        }
    }
}

/// Errors that can occur during model runtime operations.
#[derive(Debug, Error)]
pub enum ModelRuntimeError {
    /// The requested model was not found in the catalog.
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    /// A model is currently loading; try again later.
    /// Callers should return 503 Service Unavailable.
    #[error("Model is loading, try again")]
    ModelLoading,

    /// Failed to spawn the model server process.
    #[error("Failed to start model: {0}")]
    SpawnFailed(String),

    /// The model server failed its health check.
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),

    /// The model file was not found on disk.
    #[error("Model file not found: {0}")]
    ModelFileNotFound(String),

    /// Internal error during runtime operations.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl ModelRuntimeError {
    /// Returns true if this error indicates a temporary condition
    /// where retrying may succeed.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::ModelLoading)
    }

    /// Returns a suggested HTTP status code for this error.
    #[must_use]
    pub const fn suggested_status_code(&self) -> u16 {
        match self {
            Self::ModelLoading => 503,
            Self::ModelNotFound(_) | Self::ModelFileNotFound(_) => 404,
            Self::SpawnFailed(_) | Self::HealthCheckFailed(_) | Self::Internal(_) => 500,
        }
    }
}

/// Port for managing model runtime (ensuring models are running).
///
/// This is the primary interface the proxy uses to get a running
/// model server. Implementations handle:
/// - Model resolution (name → file path)
/// - Process lifecycle (start, stop, health check)
/// - Context size management
/// - Single-swap or concurrent strategies
#[async_trait]
pub trait ModelRuntimePort: Send + Sync + fmt::Debug {
    /// Ensure a model is running and ready to serve requests.
    ///
    /// This method:
    /// 1. Resolves the model name to a database entry
    /// 2. Checks if the model is already running with the correct context
    /// 3. Starts or restarts the model if needed
    /// 4. Waits for the health check to pass
    /// 5. Returns the target information for routing
    ///
    /// # Arguments
    ///
    /// * `model_name` - Name or alias of the model to run
    /// * `num_ctx` - Optional context size override from request
    /// * `default_ctx` - Default context size if not specified
    ///
    /// # Errors
    ///
    /// Returns `ModelRuntimeError` if the model cannot be started.
    async fn ensure_model_running(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError>;

    /// Get information about the currently running model, if any.
    ///
    /// Returns `None` if no model is currently running.
    async fn current_model(&self) -> Option<RunningTarget>;

    /// Stop the currently running model.
    ///
    /// This is primarily for cleanup/shutdown scenarios.
    async fn stop_current(&self) -> Result<(), ModelRuntimeError>;
}
