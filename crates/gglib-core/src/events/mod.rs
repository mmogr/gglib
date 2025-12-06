//! Canonical event union for all cross-adapter events.
//!
//! This module is the single source of truth for events used by Tauri listeners,
//! SSE handlers, and backend emitters.
//!
//! # Structure
//!
//! - `app` - Application-level events (model added/removed/updated)
//! - `download` - Download progress and completion events
//! - `server` - Model server lifecycle events
//!
//! # Wire Format
//!
//! Events are serialized with a `type` tag for TypeScript compatibility:
//!
//! ```json
//! { "type": "server_started", "modelName": "Llama-2-7B", "port": 8080 }
//! ```

mod app;
mod download;
mod server;

use serde::{Deserialize, Serialize};

// Re-export event types
pub use app::ModelSummary;
pub use server::ServerSnapshotEntry;

/// Canonical event types for all adapters.
///
/// This enum unifies server, download, and model events into a single
/// discriminated union. Each variant includes all necessary context
/// for the event to be self-describing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    // ========== Server Events ==========
    /// A model server has started and is ready to accept requests.
    ServerStarted {
        /// ID of the model being served.
        #[serde(rename = "modelId")]
        model_id: i64,
        /// Name of the model being served.
        #[serde(rename = "modelName")]
        model_name: String,
        /// Port the server is listening on.
        port: u16,
    },

    /// A model server has been stopped (clean shutdown).
    ServerStopped {
        /// ID of the model that was being served.
        #[serde(rename = "modelId")]
        model_id: i64,
        /// Name of the model that was being served.
        #[serde(rename = "modelName")]
        model_name: String,
    },

    /// A model server encountered an error.
    ServerError {
        /// ID of the model being served (if known).
        #[serde(rename = "modelId")]
        model_id: Option<i64>,
        /// Name of the model being served.
        #[serde(rename = "modelName")]
        model_name: String,
        /// Error description.
        error: String,
    },

    /// Snapshot of all currently running servers.
    ServerSnapshot {
        /// List of currently running servers.
        servers: Vec<ServerSnapshotEntry>,
    },

    // ========== Download Events ==========
    /// A download has started.
    DownloadStarted {
        /// Canonical download ID.
        id: String,
        /// Display name for the download.
        #[serde(rename = "displayName")]
        display_name: String,
    },

    /// Progress update for a download.
    DownloadProgress {
        /// Canonical download ID.
        id: String,
        /// Bytes downloaded so far.
        downloaded: u64,
        /// Total bytes to download.
        total: u64,
        /// Current speed in bytes per second.
        #[serde(rename = "speedBps")]
        speed_bps: f64,
        /// Estimated time remaining in seconds.
        #[serde(rename = "etaSeconds")]
        eta_seconds: f64,
        /// Progress percentage (0.0 - 100.0).
        percentage: f64,
    },

    /// A download has completed successfully.
    DownloadCompleted {
        /// Canonical download ID.
        id: String,
        /// Optional success message.
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },

    /// A download has failed.
    DownloadFailed {
        /// Canonical download ID.
        id: String,
        /// Error description.
        error: String,
    },

    /// A download was cancelled.
    DownloadCancelled {
        /// Canonical download ID.
        id: String,
    },

    // ========== Model Events ==========
    /// A model was added to the library.
    ModelAdded {
        /// Summary of the added model.
        model: ModelSummary,
    },

    /// A model was removed from the library.
    ModelRemoved {
        /// ID of the removed model.
        #[serde(rename = "modelId")]
        model_id: i64,
    },

    /// A model was updated in the library.
    ModelUpdated {
        /// Summary of the updated model.
        model: ModelSummary,
    },
}

impl AppEvent {
    /// Get the event name for wire protocols.
    ///
    /// This provides consistent event naming across Tauri and SSE transports.
    pub const fn event_name(&self) -> &'static str {
        match self {
            Self::ServerStarted { .. } => "server:started",
            Self::ServerStopped { .. } => "server:stopped",
            Self::ServerError { .. } => "server:error",
            Self::ServerSnapshot { .. } => "server:snapshot",
            Self::DownloadStarted { .. } => "download:started",
            Self::DownloadProgress { .. } => "download:progress",
            Self::DownloadCompleted { .. } => "download:completed",
            Self::DownloadFailed { .. } => "download:failed",
            Self::DownloadCancelled { .. } => "download:cancelled",
            Self::ModelAdded { .. } => "model:added",
            Self::ModelRemoved { .. } => "model:removed",
            Self::ModelUpdated { .. } => "model:updated",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let event = AppEvent::server_started(1, "Llama-2-7B", 8080);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"server_started\""));
        assert!(json.contains("\"modelName\":\"Llama-2-7B\""));
        assert!(json.contains("\"port\":8080"));
    }

    #[test]
    fn test_event_names() {
        assert_eq!(
            AppEvent::server_started(1, "test", 8080).event_name(),
            "server:started"
        );
        assert_eq!(
            AppEvent::download_started("id", "name").event_name(),
            "download:started"
        );
        assert_eq!(AppEvent::model_removed(1).event_name(), "model:removed");
    }
}
