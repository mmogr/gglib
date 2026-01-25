//! Shared types for process management.

use serde::Serialize;
use std::path::PathBuf;
use tokio::process::Child;

/// Configuration for spawning a llama-server process.
#[derive(Debug, Clone)]
pub struct SpawnConfig {
    /// Unique model identifier
    pub model_id: u32,
    /// Display name of the model
    pub model_name: String,
    /// Path to the model file
    pub model_path: PathBuf,
    /// Context window size (uses model default if None)
    pub context_size: Option<u64>,
    /// Specific port to use (auto-allocates if None)
    pub port: Option<u16>,
    /// Enable Jinja templating for chat formats
    pub jinja: bool,
    /// Reasoning format override (e.g., "deepseek-r1")
    pub reasoning_format: Option<String>,
}

impl SpawnConfig {
    /// Create a minimal spawn configuration with defaults.
    pub fn new(model_id: u32, model_name: String, model_path: PathBuf) -> Self {
        Self {
            model_id,
            model_name,
            model_path,
            context_size: None,
            port: None,
            jinja: false,
            reasoning_format: None,
        }
    }

    /// Set the context size.
    pub fn with_context_size(mut self, size: u64) -> Self {
        self.context_size = Some(size);
        self
    }

    /// Set the port.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Enable Jinja templating.
    pub fn with_jinja(mut self) -> Self {
        self.jinja = true;
        self
    }

    /// Set the reasoning format.
    pub fn with_reasoning_format(mut self, format: String) -> Self {
        self.reasoning_format = Some(format);
        self
    }
}

/// Information about a running model server
#[derive(Debug, Clone, Serialize)]
pub struct ServerInfo {
    /// Model ID being served
    pub model_id: u32,
    /// Model name
    pub model_name: String,
    /// Process ID
    pub pid: u32,
    /// Port the server is listening on
    pub port: u16,
    /// Unix timestamp when server was started
    pub started_at: u64,
    /// Unix timestamp of last known activity
    pub last_used: u64,
    /// Whether the server is healthy (responding to requests)
    pub healthy: bool,
    /// Context size being used
    pub context_size: Option<u64>,
}

impl ServerInfo {
    /// Create a new `ServerInfo`
    pub fn new(
        model_id: u32,
        model_name: String,
        pid: u32,
        port: u16,
        started_at: u64,
        context_size: Option<u64>,
    ) -> Self {
        Self {
            model_id,
            model_name,
            pid,
            port,
            started_at,
            last_used: started_at,
            healthy: true,
            context_size,
        }
    }

    /// Update the last used timestamp
    pub fn touch(&mut self) {
        self.last_used = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
}

/// Running process with metadata
pub struct RunningProcess {
    pub info: ServerInfo,
    pub child: Child,
}

impl RunningProcess {
    pub fn new(info: ServerInfo, child: Child) -> Self {
        Self { info, child }
    }
}
