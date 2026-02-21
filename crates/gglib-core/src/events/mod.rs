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
    /// Voice pipeline state changed (idle → listening → recording → …).
    VoiceStateChanged {
        /// Lowercase state label: `"idle"`, `"listening"`, `"recording"`,
        /// `"transcribing"`, `"thinking"`, `"speaking"`, or `"error"`.
        state: String,
    },

    /// Speech transcript produced by the STT engine.
    VoiceTranscript {
        /// Transcript text.
        text: String,
        /// Whether this is a final (committed) transcript.
        #[serde(rename = "isFinal")]
        is_final: bool,
    },

    /// TTS playback has started.
    VoiceSpeakingStarted,

    /// TTS playback has finished.
    VoiceSpeakingFinished,

    /// Microphone audio level sample (0.0 – 1.0) for UI visualisation.
    ///
    /// Throttled to ≤ 20 fps at the SSE bridge before entering the bus.
    VoiceAudioLevel {
        /// Normalised audio level in `[0.0, 1.0]`.
        level: f32,
    },

    /// Voice pipeline encountered a non-fatal error.
    VoiceError {
        /// Human-readable error message.
        message: String,
    },

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
            Self::VoiceStateChanged { .. } => "voice:state-changed",
            Self::VoiceTranscript { .. } => "voice:transcript",
            Self::VoiceSpeakingStarted => "voice:speaking-started",
            Self::VoiceSpeakingFinished => "voice:speaking-finished",
            Self::VoiceAudioLevel { .. } => "voice:audio-level",
            Self::VoiceError { .. } => "voice:error",
            Self::VoiceModelDownloadProgress { .. } => "voice:model-download-progress",
        }
    }
}

impl AppEvent {
    /// Create a [`VoiceStateChanged`] event.
    pub fn voice_state_changed(state: impl Into<String>) -> Self {
        Self::VoiceStateChanged {
            state: state.into(),
        }
    }

    /// Create a [`VoiceTranscript`] event.
    pub fn voice_transcript(text: impl Into<String>, is_final: bool) -> Self {
        Self::VoiceTranscript {
            text: text.into(),
            is_final,
        }
    }

    /// Create a [`VoiceSpeakingStarted`] event.
    pub const fn voice_speaking_started() -> Self {
        Self::VoiceSpeakingStarted
    }

    /// Create a [`VoiceSpeakingFinished`] event.
    pub const fn voice_speaking_finished() -> Self {
        Self::VoiceSpeakingFinished
    }

    /// Create a [`VoiceAudioLevel`] event.
    pub const fn voice_audio_level(level: f32) -> Self {
        Self::VoiceAudioLevel { level }
    }

    /// Create a [`VoiceError`] event.
    pub fn voice_error(message: impl Into<String>) -> Self {
        Self::VoiceError {
            message: message.into(),
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

    /// Lock down voice event names to prevent frontend subscription mismatches.
    ///
    /// Both the Serde `type` tag (SSE/WebUI path) and the `event_name()` return
    /// (Tauri IPC path) are validated here.
    ///
    /// If this test fails, update `VOICE_EVENT_NAMES` in
    /// `src/services/transport/events/eventNames.ts` to match.
    #[test]
    fn voice_event_names_are_stable() {
        let cases = vec![
            (AppEvent::voice_state_changed("idle"), "voice:state-changed"),
            (
                AppEvent::voice_transcript("hello", true),
                "voice:transcript",
            ),
            (AppEvent::voice_speaking_started(), "voice:speaking-started"),
            (
                AppEvent::voice_speaking_finished(),
                "voice:speaking-finished",
            ),
            (AppEvent::voice_audio_level(0.5), "voice:audio-level"),
            (AppEvent::voice_error("oops"), "voice:error"),
        ];
        for (event, expected_name) in cases {
            assert_eq!(event.event_name(), expected_name);
        }

        // Also assert Serde type tags match the frontend SSE routing prefix.
        let json = serde_json::to_string(&AppEvent::voice_state_changed("idle")).unwrap();
        assert!(
            json.contains("\"type\":\"voice_state_changed\""),
            "bad serde tag: {json}"
        );

        let json = serde_json::to_string(&AppEvent::voice_audio_level(0.5)).unwrap();
        assert!(
            json.contains("\"type\":\"voice_audio_level\""),
            "bad serde tag: {json}"
        );

        let json = serde_json::to_string(&AppEvent::voice_error("oops")).unwrap();
        assert!(
            json.contains("\"type\":\"voice_error\""),
            "bad serde tag: {json}"
        );
    }
}
