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
    /// True when this instance was freshly spawned (restart or cold start).
    pub just_started: bool,
    /// Whether llama-server's disk slot save/restore can actually resume this
    /// model, i.e. its KV memory retains the full token history.
    ///
    /// False for sliding-window, hybrid, and recurrent architectures (see
    /// [`crate::domain::kv_memory_is_partial`]): the slot file carries KV
    /// state and tokens but not the server's context checkpoints, so a
    /// restore leaves the slot unable to resume and llama-server re-prefills
    /// the whole prompt. Callers skip the disk slot layer when this is false
    /// and let the in-RAM prompt cache — which does keep checkpoints — handle
    /// conversation switching.
    pub slot_restore_supported: bool,
}

impl RunningTarget {
    /// Create a new `RunningTarget` for a local server.
    ///
    /// `slot_restore_supported` defaults to `true` (the full-attention case);
    /// callers that know the model's KV memory shape narrow it with
    /// [`Self::with_slot_restore_supported`].
    #[must_use]
    pub fn local(
        port: u16,
        model_id: u32,
        model_name: String,
        effective_ctx: u64,
        just_started: bool,
    ) -> Self {
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            port,
            model_id,
            model_name,
            effective_ctx,
            just_started,
            slot_restore_supported: true,
        }
    }

    /// Set whether disk slot restore can resume this model.
    #[must_use]
    pub const fn with_slot_restore_supported(mut self, supported: bool) -> Self {
        self.slot_restore_supported = supported;
        self
    }
}

/// Errors that can occur during model runtime operations.
#[derive(Clone, Debug, Error)]
pub enum ModelRuntimeError {
    /// The requested model was not found in the catalog.
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    /// A model is currently loading; try again later.
    /// Callers should return 503 Service Unavailable.
    #[error("Model is loading, try again")]
    ModelLoading,

    /// Retryable: another caller is loading the same model, we waited too long for contention to clear.
    #[error("Contention timeout: {0}")]
    ContentionTimeout(String),

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
        matches!(self, Self::ModelLoading | Self::ContentionTimeout(_))
    }

    /// Returns a suggested HTTP status code for this error.
    #[must_use]
    pub const fn suggested_status_code(&self) -> u16 {
        match self {
            Self::ModelLoading | Self::ContentionTimeout(_) => 503,
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
