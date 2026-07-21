//! Process runner trait definition.
//!
//! This port defines the interface for managing model server processes.
//! Implementations handle all process lifecycle details internally.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::ProcessError;
use crate::domain::InferenceConfig;

/// Configuration for starting a model server.
///
/// This is an intent-based configuration — it expresses what the caller
/// wants, not how the server should be started. All typed fields are
/// handled by `build_and_spawn()`; `extra_args` is an escape hatch for
/// flags not yet promoted to first-class fields.
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
    /// Enable Jinja templating for chat formats.
    pub jinja: bool,
    /// Reasoning format override (e.g., `"deepseek"`, `"none"`).
    pub reasoning_format: Option<String>,
    /// Number of MTP draft tokens to speculate ahead (`--spec-draft-n-max`).
    ///
    /// `None` means MTP speculative decoding is disabled.  When `Some(n)`,
    /// `--spec-type draft-mtp` and `--spec-draft-n-max n` are passed to
    /// llama-server.  Recommended value: `2` (Unsloth default).
    pub spec_draft_n_max: Option<u32>,
    /// Minimum acceptance probability for MTP draft tokens (`--spec-draft-p-min`).
    ///
    /// Only meaningful when `spec_draft_n_max` is `Some`.  Skipping low-confidence
    /// draft tokens is especially important on Apple Silicon (Metal) to avoid
    /// throughput regression.  Recommended value: `0.75`.
    pub spec_draft_p_min: Option<f32>,
    /// Inference sampling parameters (temperature, `top_p`, etc.).
    pub inference_config: Option<InferenceConfig>,
    /// Additional server-specific options (escape hatch).
    pub extra_args: Vec<String>,
    /// Directory for llama-server KV cache slot persistence (`--slot-save-path`).
    ///
    /// `None` means the disk slot-persistence feature is disabled — no
    /// `--slot-save-path` flag is passed. Independent of [`Self::cache_ram_mb`]
    /// / [`Self::cache_reuse`]: llama-server's own host-RAM prompt cache can be
    /// tuned (or left at its built-in default) regardless of whether disk
    /// persistence is on.
    pub slot_save_path: Option<PathBuf>,
    /// RAM budget in MiB for llama-server's own host-RAM prompt cache
    /// (`--cache-ram`).
    ///
    /// `None` means no explicit flag is passed — llama-server's own built-in
    /// default (8192 MiB) applies. `Some(n)` passes `--cache-ram n` directly;
    /// `Some(0)` disables the cache.
    pub cache_ram_mb: Option<u64>,
    /// Minimum chunk size in tokens for KV-shift cache reuse past the first
    /// prefix divergence point (`--cache-reuse`).
    ///
    /// `None` means no flag is passed (`--cache-reuse` off, llama-server
    /// default `0`). `Some(n)` passes `--cache-reuse n`, letting llama-server
    /// salvage matching KV chunks after an edited/summarized earlier message
    /// instead of only reusing an unbroken prefix from token 0.
    pub cache_reuse: Option<u32>,
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
            jinja: false,
            reasoning_format: None,
            spec_draft_n_max: None,
            spec_draft_p_min: None,
            inference_config: None,
            extra_args: Vec::new(),
            slot_save_path: None,
            cache_ram_mb: None,
            cache_reuse: None,
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

    /// Enable Jinja templating.
    #[must_use]
    pub const fn with_jinja(mut self) -> Self {
        self.jinja = true;
        self
    }

    /// Set the reasoning format (e.g., `"deepseek"`, `"none"`).
    #[must_use]
    pub fn with_reasoning_format(mut self, format: String) -> Self {
        self.reasoning_format = Some(format);
        self
    }

    /// Enable MTP speculative decoding with the given draft token count.
    ///
    /// This causes `--spec-type draft-mtp` and `--spec-draft-n-max n` to be
    /// passed to llama-server.  Call [`Self::with_spec_draft_p_min`] to also
    /// set the acceptance probability threshold (defaults to 0.75).
    #[must_use]
    pub const fn with_spec_draft_n_max(mut self, n: u32) -> Self {
        self.spec_draft_n_max = Some(n);
        self
    }

    /// Set the minimum acceptance probability for MTP draft tokens.
    ///
    /// Has no effect unless `spec_draft_n_max` is also set.  Recommended
    /// value is `0.75`; lower values trade quality for speed.
    #[must_use]
    pub const fn with_spec_draft_p_min(mut self, p: f32) -> Self {
        self.spec_draft_p_min = Some(p);
        self
    }

    /// Set inference sampling parameters.
    #[must_use]
    pub const fn with_inference_config(mut self, config: InferenceConfig) -> Self {
        self.inference_config = Some(config);
        self
    }

    /// Add extra arguments to pass to the server.
    #[must_use]
    pub fn with_extra_args(mut self, args: Vec<String>) -> Self {
        self.extra_args = args;
        self
    }

    /// Set the KV cache slot-save directory (`--slot-save-path`).
    ///
    /// `None` disables the disk slot-persistence feature (no
    /// `--slot-save-path` flag emitted). Independent of
    /// [`Self::with_cache_ram_mb`] / [`Self::with_cache_reuse`].
    #[must_use]
    pub fn with_slot_save_path(mut self, path: Option<PathBuf>) -> Self {
        self.slot_save_path = path;
        self
    }

    /// Set the RAM budget (in MiB) for llama-server's own host-RAM prompt
    /// cache (`--cache-ram`). `None` leaves llama-server's built-in default.
    #[must_use]
    pub const fn with_cache_ram_mb(mut self, mb: u64) -> Self {
        self.cache_ram_mb = Some(mb);
        self
    }

    /// Set the minimum chunk size (in tokens) for KV-shift cache reuse
    /// (`--cache-reuse`). `None` leaves the feature off.
    #[must_use]
    pub const fn with_cache_reuse(mut self, n: u32) -> Self {
        self.cache_reuse = Some(n);
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
