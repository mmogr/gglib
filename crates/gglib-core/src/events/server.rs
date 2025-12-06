//! Model server lifecycle events.

use serde::{Deserialize, Serialize};

use super::AppEvent;

/// Entry in a server snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSnapshotEntry {
    /// Model ID being served.
    pub model_id: i64,
    /// Model name.
    pub model_name: String,
    /// Port the server is listening on.
    pub port: u16,
    /// Unix timestamp (seconds) when started.
    pub started_at: u64,
    /// Whether the server is healthy.
    pub healthy: bool,
}

impl AppEvent {
    /// Create a server started event.
    pub fn server_started(model_id: i64, model_name: impl Into<String>, port: u16) -> Self {
        Self::ServerStarted {
            model_id,
            model_name: model_name.into(),
            port,
        }
    }

    /// Create a server stopped event.
    pub fn server_stopped(model_id: i64, model_name: impl Into<String>) -> Self {
        Self::ServerStopped {
            model_id,
            model_name: model_name.into(),
        }
    }

    /// Create a server error event.
    pub fn server_error(
        model_id: Option<i64>,
        model_name: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self::ServerError {
            model_id,
            model_name: model_name.into(),
            error: error.into(),
        }
    }

    /// Create a server snapshot event.
    pub fn server_snapshot(servers: Vec<ServerSnapshotEntry>) -> Self {
        Self::ServerSnapshot { servers }
    }
}
