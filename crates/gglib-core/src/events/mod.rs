#![doc = include_str!("README.md")]
mod app;
mod remote;
mod server;

#[cfg(test)]
mod app_event_tests;

use serde::{Deserialize, Serialize};

use crate::ports::RuntimeErrorEnvelope;

// Re-export event types
pub use app::ModelSummary;

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
        /// Serde always sends the key — a model that is not known arrives as
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

    // ========== Download Events ==========
    /// The download queue's snapshot, and the events that say a download
    /// ended.
    ///
    /// Wraps `DownloadEvent` verbatim.
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

    /// A model left the library.
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

    // ========== Remote tunnel events (ADR 0012) ==========
    /// The remote tunnel is up and has a ticket. The ticket itself is
    /// never on the event stream — any local GUI client can read it — so
    /// this carries the same twelve-character fingerprint the logs use.
    RemoteEnabled {
        /// Fingerprint of the ticket this session minted.
        #[serde(rename = "ticketFingerprint")]
        ticket_fingerprint: String,
    },

    /// The remote tunnel was taken down; nothing answers its ticket until
    /// `enable` puts the same one back.
    RemoteDisabled,

    /// A device redeemed the pairing code and now holds the key.
    RemotePaired {
        /// The tunnel edge's fingerprint for that device, when it sent one.
        peer: Option<String>,
    },

    /// This machine's connect side is up: a local port that is the remote
    /// proxy.
    RemoteJoined {
        /// The loopback port the tunnel is bound to on this machine.
        port: u16,
    },

    /// The connect side was taken down.
    RemoteDisconnected,

    /// The far machine has been away past the grace. The port stays bound
    /// and is still being dialled; this says so, so a client is not left
    /// reading "connected" over nothing.
    RemoteAway {
        /// The loopback port that is still bound.
        port: u16,
    },

    /// The far machine answered again after being away.
    RemoteBack {
        /// The loopback port, unchanged.
        port: u16,
    },
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
