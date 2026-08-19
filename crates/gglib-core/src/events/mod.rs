#![doc = include_str!("README.md")]
mod app;
mod server;

use serde::{Deserialize, Serialize};

use crate::ports::RuntimeErrorEnvelope;

// Re-export event types
pub use app::ModelSummary;
pub use server::{NoopServerEvents, ServerEvents, ServerSnapshotEntry, ServerSummary};

// Import download types for AppEvent::Download wrapper
use crate::download::DownloadEvent;

/// Canonical event types for all adapters.
///
/// This enum unifies server, download, and model events into a single
/// discriminated union. Each variant includes all necessary context
/// for the event to be self-describing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    // ========== Server Events ==========
    /// A model server has started and is ready to accept requests.
    ServerStarted {
        /// ID of the model being served.
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
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
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
        #[serde(rename = "modelId")]
        model_id: i64,
        /// Name of the model that was being served.
        #[serde(rename = "modelName")]
        model_name: String,
    },

    /// A model server encountered an error.
    ServerError {
        /// ID of the model being served (if known).
        ///
        /// Serde always sends the key — an unparseable model ID arrives as
        /// `null`, not as an absent field.
        #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
        #[serde(rename = "modelId")]
        model_id: Option<i64>,
        /// Name of the model being served.
        #[serde(rename = "modelName")]
        model_name: String,
        /// Structured error detail (message, type discriminant, retryable
        /// flag), mirroring the HTTP layer's `ErrorResponse` shape.
        error: RuntimeErrorEnvelope,
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
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
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
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
        #[serde(rename = "modelId")]
        model_id: i64,
        /// Name of the model being verified.
        #[serde(rename = "modelName")]
        model_name: String,
        /// Name of the shard being verified.
        #[serde(rename = "shardName")]
        shard_name: String,
        /// Bytes processed so far.
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
        #[serde(rename = "bytesProcessed")]
        bytes_processed: u64,
        /// Total bytes to process.
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
        #[serde(rename = "totalBytes")]
        total_bytes: u64,
    },

    /// Model verification completed.
    VerificationComplete {
        /// ID of the verified model.
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
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
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
        #[serde(rename = "serverId")]
        server_id: i64,
        /// ID of the model being served.
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
        #[serde(rename = "modelId")]
        model_id: i64,
        /// New health status.
        status: crate::ports::ServerHealthStatus,
        /// Optional detail message (e.g., error description).
        #[cfg_attr(feature = "ts-bindings", ts(optional))]
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        /// Unix timestamp in milliseconds when status changed.
        #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
        timestamp: u64,
    },

    // ========== Proxy Events ==========
    /// The OpenAI-compatible proxy has started.
    ProxyStarted {
        /// Port the proxy is listening on.
        port: u16,
    },

    /// The proxy has been stopped (clean shutdown).
    ProxyStopped,

    /// The proxy crashed (task exited without cancellation).
    ProxyCrashed,
}

impl AppEvent {
    /// Colon-separated event names.
    ///
    /// **Not what goes on the wire.** `AppEvent` is `#[serde(tag = "type",
    /// rename_all = "snake_case")]`, so SSE carries `download`/`model_added`;
    /// these `download:started` spellings are the Tauri-bus vocabulary from
    /// before the GUI backend moved into the daemon, and the Tauri event
    /// branch that subscribed to them is gone.
    ///
    /// Nothing outside this module's tests calls it. Retiring it belongs with
    /// the rest of the residual-Rust sweep, not here.
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
            Self::ProxyStarted { .. } => "proxy:started",
            Self::ProxyStopped => "proxy:stopped",
            Self::ProxyCrashed => "proxy:crashed",
        }
    }
}

impl AppEvent {
    /// Create a [`AppEvent::ProxyStarted`] event.
    pub const fn proxy_started(port: u16) -> Self {
        Self::ProxyStarted { port }
    }

    /// Create a [`AppEvent::ProxyStopped`] event.
    pub const fn proxy_stopped() -> Self {
        Self::ProxyStopped
    }

    /// Create a [`AppEvent::ProxyCrashed`] event.
    pub const fn proxy_crashed() -> Self {
        Self::ProxyCrashed
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
        assert_eq!(AppEvent::model_removed(1).event_name(), "model:removed");
        // The download names are covered exhaustively by
        // `download_event_names_are_stable` below.
    }

    /// Lock down the colon-separated download event names.
    ///
    /// This guards [`AppEvent::event_name`] against silent renames, and that
    /// is all it guards: none of these five strings appears anywhere in the
    /// frontend. The names the GUI actually validates are the `snake_case`
    /// serde variants, in `src/services/decoders/downloadEvent.ts`.
    ///
    /// It was written when the frontend did subscribe to these, over the
    /// Tauri bus, and its doc pointed at `eventNames.ts` until #833 deleted
    /// that file. Pointing it at `getEventCategory` instead — as an earlier
    /// pass here did — is no better: that allowlist matches the serde tag, so
    /// updating it in response to this test failing would be a no-op.
    ///
    /// Context: downloads started but the progress UI never appeared, because
    /// the frontend listened for the wrong event names.
    #[test]
    fn download_event_names_are_stable() {
        let cases = [
            (DownloadEvent::started("id"), "download:started"),
            (
                DownloadEvent::progress("id", 50, 100, Some(1024.0), Some(10.0)),
                "download:progress",
            ),
            (
                DownloadEvent::completed("id", None::<String>),
                "download:completed",
            ),
            (DownloadEvent::failed("id", "error"), "download:failed"),
            (DownloadEvent::cancelled("id"), "download:cancelled"),
        ];

        for (event, expected_name) in cases {
            assert_eq!(AppEvent::Download { event }.event_name(), expected_name);
        }
    }
}
