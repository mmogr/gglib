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
//! - `mcp` - MCP server lifecycle events
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
mod mcp;
mod server;

use serde::{Deserialize, Serialize};

use crate::ports::McpErrorInfo;

// Re-export event types
pub use app::ModelSummary;
pub use mcp::McpServerSummary;
pub use server::{NoopServerEvents, ServerEvents, ServerSnapshotEntry, ServerSummary};

// Import download types for AppEvent::Download wrapper
use crate::download::DownloadEvent;

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
    /// Download lifecycle + progress events (including shard progress).
    ///
    /// Wraps `DownloadEvent` verbatim to preserve all detail including
    /// shard-specific progress information.
    #[serde(rename = "download")]
    Download {
        /// The download event payload.
        event: DownloadEvent,
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

    // ========== Verification Events ==========
    /// Model verification progress update.
    VerificationProgress {
        /// ID of the model being verified.
        #[serde(rename = "modelId")]
        model_id: i64,
        /// Name of the model being verified.
        #[serde(rename = "modelName")]
        model_name: String,
        /// Name of the shard being verified.
        #[serde(rename = "shardName")]
        shard_name: String,
        /// Bytes processed so far.
        #[serde(rename = "bytesProcessed")]
        bytes_processed: u64,
        /// Total bytes to process.
        #[serde(rename = "totalBytes")]
        total_bytes: u64,
    },

    /// Model verification completed.
    VerificationComplete {
        /// ID of the verified model.
        #[serde(rename = "modelId")]
        model_id: i64,
        /// Name of the verified model.
        #[serde(rename = "modelName")]
        model_name: String,
        /// Overall health status.
        #[serde(rename = "overallHealth")]
        overall_health: crate::services::OverallHealth,
    },

    /// Server health status has changed.
    ///
    /// Emitted by continuous monitoring when a server's health state changes.
    ServerHealthChanged {
        /// Unique server instance identifier.
        #[serde(rename = "serverId")]
        server_id: i64,
        /// ID of the model being served.
        #[serde(rename = "modelId")]
        model_id: i64,
        /// New health status.
        status: crate::ports::ServerHealthStatus,
        /// Optional detail message (e.g., error description).
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        /// Unix timestamp in milliseconds when status changed.
        timestamp: u64,
    },

    // ========== MCP Server Events ==========
    /// An MCP server was added to the configuration.
    McpServerAdded {
        /// Summary of the added server.
        server: McpServerSummary,
    },

    /// An MCP server was removed from the configuration.
    McpServerRemoved {
        /// ID of the removed server.
        #[serde(rename = "serverId")]
        server_id: i64,
    },

    /// An MCP server has started and is ready.
    McpServerStarted {
        /// ID of the server.
        #[serde(rename = "serverId")]
        server_id: i64,
        /// Name of the server.
        #[serde(rename = "serverName")]
        server_name: String,
    },

    /// An MCP server has been stopped.
    McpServerStopped {
        /// ID of the server.
        #[serde(rename = "serverId")]
        server_id: i64,
        /// Name of the server.
        #[serde(rename = "serverName")]
        server_name: String,
    },

    /// An MCP server encountered an error.
    McpServerError {
        /// User-safe error information.
        error: McpErrorInfo,
    },

    // ========== Voice Events ==========
    /// Download progress for a voice model (STT / TTS / VAD).
    ///
    /// Emitted by `VoiceService` during `download_stt_model`,
    /// `download_tts_model`, and `download_vad_model` calls so that
    /// SSE subscribers receive live progress without Tauri's `app.emit()`.
    VoiceModelDownloadProgress {
        /// Identifier of the model being downloaded (e.g. `"base.en"`).
        #[serde(rename = "modelId")]
        model_id: String,
        /// Bytes downloaded so far.
        #[serde(rename = "bytesDownloaded")]
        bytes_downloaded: u64,
        /// Total bytes to download (`0` if the server did not send
        /// `Content-Length`).
        #[serde(rename = "totalBytes")]
        total_bytes: u64,
        /// Progress percentage (`0.0`–`100.0`; `0.0` when total is unknown).
        percent: f64,
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
            Self::ServerHealthChanged { .. } => "server:health_changed",
            Self::Download { event } => event.event_name(),
            Self::ModelAdded { .. } => "model:added",
            Self::ModelRemoved { .. } => "model:removed",
            Self::ModelUpdated { .. } => "model:updated",
            Self::VerificationProgress { .. } => "verification:progress",
            Self::VerificationComplete { .. } => "verification:complete",
            Self::McpServerAdded { .. } => "mcp:added",
            Self::McpServerRemoved { .. } => "mcp:removed",
            Self::McpServerStarted { .. } => "mcp:started",
            Self::McpServerStopped { .. } => "mcp:stopped",
            Self::McpServerError { .. } => "mcp:error",
            Self::VoiceModelDownloadProgress { .. } => "voice:model_download_progress",
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

    /// Lock down download event names to prevent frontend subscription mismatches.
    ///
    /// This test protects the contract between backend event emission and frontend
    /// Tauri event subscription. If this test fails, update the `DOWNLOAD_EVENT_NAMES`
    /// constant in src/services/transport/events/eventNames.ts to match.
    ///
    /// Context: Issue where Tauri GUI downloads started but progress UI never appeared
    /// because frontend listened to wrong event names.
    #[test]
    fn download_event_names_are_stable() {
        let cases = vec![
            (AppEvent::download_started("id", "name"), "download:started"),
            (
                AppEvent::download_progress("id", 50, 100, 1024.0, 10.0, 50.0),
                "download:progress",
            ),
            (
                AppEvent::download_completed("id", None),
                "download:completed",
            ),
            (AppEvent::download_failed("id", "error"), "download:failed"),
            (AppEvent::download_cancelled("id"), "download:cancelled"),
        ];

        for (event, expected_name) in cases {
            assert_eq!(event.event_name(), expected_name);
        }
    }
}
