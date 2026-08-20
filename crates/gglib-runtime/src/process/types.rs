//! Shared types for process management.

use serde::Serialize;
use tokio::process::Child;

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
}

impl ServerInfo {
    /// Create a new `ServerInfo`
    pub fn new(model_id: u32, model_name: String, pid: u32, port: u16, started_at: u64) -> Self {
        Self {
            model_id,
            model_name,
            pid,
            port,
            started_at,
        }
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
