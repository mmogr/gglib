//! Model server lifecycle events.

use serde::{Deserialize, Serialize};

use super::AppEvent;

/// Summary of a running server for event emission.
///
/// This is a lightweight representation used by the `ServerEvents` port
/// to decouple lifecycle logic from transport-specific implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSummary {
    /// Unique server instance ID.
    pub id: String,
    /// Model ID being served.
    pub model_id: String,
    /// Model name.
    pub model_name: String,
    /// Port the server is listening on.
    pub port: u16,
    /// Health status (None = unknown/pending).
    pub healthy: Option<bool>,
}

impl ServerSummary {
    /// Parse the `model_id` string as a u32.
    ///
    /// Returns `None` if parsing fails.
    ///
    /// Pure helper: adapters decide how to handle/log parse failures.
    pub fn parsed_model_id(&self) -> Option<u32> {
        self.model_id.parse::<u32>().ok()
    }
}

/// Port for emitting server lifecycle events.
///
/// This trait decouples the core server lifecycle logic from transport-specific
/// event emission (Tauri events, SSE, logging, etc.). Implementations convert
/// `ServerSummary` to their native event format.
///
/// # Design
///
/// - **Object-safe**: Uses `&self` for dynamic dispatch via `Arc<dyn ServerEvents>`
/// - **Fire-and-forget**: Methods don't return `Result` — adapters handle errors internally
/// - **Generic**: No knowledge of Tauri/Axum/CLI specifics
///
/// # Example
///
/// ```rust
/// use gglib_core::events::{ServerEvents, ServerSummary};
///
/// struct LoggingEvents;
///
/// impl ServerEvents for LoggingEvents {
///     fn started(&self, server: &ServerSummary) {
///         println!("Server {} started on port {}", server.model_name, server.port);
///     }
///     fn stopping(&self, server: &ServerSummary) {
///         println!("Stopping server {}", server.model_name);
///     }
///     fn stopped(&self, server: &ServerSummary) {
///         println!("Server {} stopped", server.model_name);
///     }
///     fn snapshot(&self, servers: &[ServerSummary]) {
///         println!("Server snapshot: {} running", servers.len());
///     }
///     fn error(&self, server: &ServerSummary, error: &str) {
///         eprintln!("Server {} error: {}", server.model_name, error);
///     }
/// }
/// ```
pub trait ServerEvents: Send + Sync {
    /// Called when a server has successfully started.
    fn started(&self, server: &ServerSummary);

    /// Called just before stopping a server.
    fn stopping(&self, server: &ServerSummary);

    /// Called after a server has stopped.
    fn stopped(&self, server: &ServerSummary);

    /// Called to broadcast the current state of all running servers.
    fn snapshot(&self, servers: &[ServerSummary]);

    /// Called when a server error occurs.
    fn error(&self, server: &ServerSummary, error: &str);
}

/// No-op implementation of `ServerEvents` for testing and non-GUI contexts.
///
/// This is the default when `GuiBackend` is constructed without explicit
/// event handling (e.g., in unit tests or CLI contexts).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopServerEvents;

impl ServerEvents for NoopServerEvents {
    fn started(&self, _server: &ServerSummary) {}
    fn stopping(&self, _server: &ServerSummary) {}
    fn stopped(&self, _server: &ServerSummary) {}
    fn snapshot(&self, _servers: &[ServerSummary]) {}
    fn error(&self, _server: &ServerSummary, _error: &str) {}
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
    pub const fn server_snapshot(servers: Vec<ServerSnapshotEntry>) -> Self {
        Self::ServerSnapshot { servers }
    }
}
