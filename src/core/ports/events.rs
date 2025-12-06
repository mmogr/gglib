//! Canonical event union for all cross-adapter events.
//!
//! This is the single source of truth for events used by Tauri listeners,
//! SSE handlers, and backend emitters. Event payload changes require
//! updating both Tauri and SSE adapters in the same PR.
//!
//! # Migration Note
//!
//! This is defined in Phase 2 but fully adopted in Phase 3 (Frontend
//! Transport Unification). Until then, existing `ServerEvent` and
//! `DownloadEvent` types remain in use by adapters.

use serde::{Deserialize, Serialize};

/// Summary of a model for event payloads.
///
/// This is a lightweight representation for events — not the full `Model`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSummary {
    /// Database ID of the model.
    pub id: i64,
    /// Human-readable model name.
    pub name: String,
    /// File path to the model.
    pub file_path: String,
    /// Model architecture (e.g., "llama").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    /// Quantization type (e.g., "Q4_0").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
}

/// Canonical event types for all adapters.
///
/// This enum unifies server, download, and model events into a single
/// discriminated union. Each variant includes all necessary context
/// for the event to be self-describing.
///
/// # Wire Format
///
/// Events are serialized with a `type` tag for TypeScript compatibility:
///
/// ```json
/// { "type": "server_started", "modelName": "Llama-2-7B", "port": 8080 }
/// ```
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

    // ========== Server Event Constructors ==========

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

    // ========== Download Event Constructors ==========

    /// Create a download started event.
    pub fn download_started(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self::DownloadStarted {
            id: id.into(),
            display_name: display_name.into(),
        }
    }

    /// Create a download progress event.
    pub fn download_progress(
        id: impl Into<String>,
        downloaded: u64,
        total: u64,
        speed_bps: f64,
        eta_seconds: f64,
        percentage: f64,
    ) -> Self {
        Self::DownloadProgress {
            id: id.into(),
            downloaded,
            total,
            speed_bps,
            eta_seconds,
            percentage,
        }
    }

    /// Create a download completed event.
    pub fn download_completed(id: impl Into<String>, message: Option<String>) -> Self {
        Self::DownloadCompleted {
            id: id.into(),
            message,
        }
    }

    /// Create a download failed event.
    pub fn download_failed(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self::DownloadFailed {
            id: id.into(),
            error: error.into(),
        }
    }

    /// Create a download cancelled event.
    pub fn download_cancelled(id: impl Into<String>) -> Self {
        Self::DownloadCancelled { id: id.into() }
    }

    // ========== Model Event Constructors ==========

    /// Create a model added event.
    pub fn model_added(model: ModelSummary) -> Self {
        Self::ModelAdded { model }
    }

    /// Create a model removed event.
    pub fn model_removed(model_id: i64) -> Self {
        Self::ModelRemoved { model_id }
    }

    /// Create a model updated event.
    pub fn model_updated(model: ModelSummary) -> Self {
        Self::ModelUpdated { model }
    }
}

impl ModelSummary {
    /// Create a new model summary.
    pub fn new(
        id: i64,
        name: impl Into<String>,
        file_path: impl Into<String>,
        architecture: Option<String>,
        quantization: Option<String>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            file_path: file_path.into(),
            architecture,
            quantization,
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
