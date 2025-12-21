//! Process runner trait definition.
//!
//! This port defines the interface for managing model server processes.
//! Implementations handle all process lifecycle details internally.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::ProcessError;

/// Configuration for starting a model server.
///
/// This is an intent-based configuration — it expresses what the caller
/// wants, not how the server should be started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Database ID of the model to serve.
    pub model_id: i64,
    /// Human-readable model name.
    pub model_name: String,
    /// Path to the model file.
    pub model_path: PathBuf,
    /// Port to listen on (if None, a free port will be assigned).
    pub port: Option<u16>,
    /// Base port for allocation when port is None.
    pub base_port: u16,
    /// Context size to use (if None, use model default).
    pub context_size: Option<u64>,
    /// Number of GPU layers to offload (if None, use default).
    pub gpu_layers: Option<i32>,
    /// Additional server-specific options.
    pub extra_args: Vec<String>,
}

impl ServerConfig {
    /// Create a new server configuration with required fields.
    #[must_use]
    pub const fn new(
        model_id: i64,
        model_name: String,
        model_path: PathBuf,
        base_port: u16,
    ) -> Self {
        Self {
            model_id,
            model_name,
            model_path,
            port: None,
            base_port,
            context_size: None,
            gpu_layers: None,
            extra_args: Vec::new(),
        }
    }

    /// Set the port to listen on.
    #[must_use]
    pub const fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set the context size.
    #[must_use]
    pub const fn with_context_size(mut self, size: u64) -> Self {
        self.context_size = Some(size);
        self
    }

    /// Set the number of GPU layers.
    #[must_use]
    pub const fn with_gpu_layers(mut self, layers: i32) -> Self {
        self.gpu_layers = Some(layers);
        self
    }

    /// Add extra arguments to pass to the server.
    #[must_use]
    pub fn with_extra_args(mut self, args: Vec<String>) -> Self {
        self.extra_args = args;
        self
    }
}

/// Handle to a running server process.
///
/// This is an opaque handle that implementations use to track processes.
/// It contains enough information to identify and manage the process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessHandle {
    /// Database ID of the model being served.
    pub model_id: i64,
    /// Human-readable model name.
    pub model_name: String,
    /// Process ID (if running on local system).
    pub pid: Option<u32>,
    /// Port the server is listening on.
    pub port: u16,
    /// Unix timestamp (seconds) when the server was started.
    pub started_at: u64,
}

impl ProcessHandle {
    /// Create a new process handle.
    #[must_use]
    pub const fn new(
        model_id: i64,
        model_name: String,
        pid: Option<u32>,
        port: u16,
        started_at: u64,
    ) -> Self {
        Self {
            model_id,
            model_name,
            pid,
            port,
            started_at,
        }
    }
}

/// Health status of a running server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerHealth {
    /// Whether the server is responding to health checks.
    pub healthy: bool,
    /// Unix timestamp (seconds) of the last successful health check.
    pub last_check: Option<u64>,
    /// Context size being used by the server.
    pub context_size: Option<u64>,
    /// Optional status message.
    pub message: Option<String>,
}

impl ServerHealth {
    /// Get the current Unix timestamp in seconds.
    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Create a healthy server status.
    #[must_use]
    pub fn healthy() -> Self {
        Self {
            healthy: true,
            last_check: Some(Self::now_secs()),
            context_size: None,
            message: None,
        }
    }

    /// Create an unhealthy server status with a message.
    pub fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            healthy: false,
            last_check: Some(Self::now_secs()),
            context_size: None,
            message: Some(message.into()),
        }
    }

    /// Set the context size.
    #[must_use]
    pub const fn with_context_size(mut self, size: u64) -> Self {
        self.context_size = Some(size);
        self
    }
}

/// Process runner for managing model server processes.
///
/// This trait abstracts process management for testability and
/// potential alternative backends (local, remote, containerized).
///
/// # Design Rules
///
/// - Express **intent**, not implementation detail
/// - No CLI/Tauri/Axum concerns in signatures
/// - Must support: mock runner, remote runner, alternative inference backends
#[async_trait]
pub trait ProcessRunner: Send + Sync {
    /// Start a model server with the given configuration.
    ///
    /// Returns a handle that can be used to manage the process.
    async fn start(&self, config: ServerConfig) -> Result<ProcessHandle, ProcessError>;

    /// Stop a running server.
    ///
    /// Returns `Err(ProcessError::NotRunning)` if the process isn't running.
    async fn stop(&self, handle: &ProcessHandle) -> Result<(), ProcessError>;

    /// Check if a server is still running.
    async fn is_running(&self, handle: &ProcessHandle) -> bool;

    /// Get the health status of a running server.
    ///
    /// Returns `Err(ProcessError::NotRunning)` if the process isn't running.
    async fn health(&self, handle: &ProcessHandle) -> Result<ServerHealth, ProcessError>;

    /// List all currently running server processes.
    ///
    /// This is needed for snapshot behavior (e.g., `server:snapshot` events).
    async fn list_running(&self) -> Result<Vec<ProcessHandle>, ProcessError>;
}
