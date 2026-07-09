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

    /// Build a `ServerStarted` event from a `ServerSummary`.
    pub fn from_server_started(server: &ServerSummary) -> Self {
        let model_id = server.model_id.parse::<i64>().unwrap_or(0);
        Self::server_started(model_id, &server.model_name, server.port)
    }

    /// Build a `ServerStopped` event from a `ServerSummary`.
    pub fn from_server_stopped(server: &ServerSummary) -> Self {
        let model_id = server.model_id.parse::<i64>().unwrap_or(0);
        Self::server_stopped(model_id, &server.model_name)
    }

    /// Build a `ServerError` event from a `ServerSummary`.
    pub fn from_server_error(server: &ServerSummary, error: &str) -> Self {
        let model_id = server.model_id.parse::<i64>().ok();
        Self::server_error(model_id, &server.model_name, error)
    }

    /// Build a `ServerSnapshot` event from a slice of `ServerSummary`.
    pub fn from_server_snapshot(servers: &[ServerSummary]) -> Self {
        let started_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let entries: Vec<ServerSnapshotEntry> = servers
            .iter()
            .map(|s| ServerSnapshotEntry {
                model_id: s.model_id.parse::<i64>().unwrap_or(0),
                model_name: s.model_name.clone(),
                port: s.port,
                started_at,
                healthy: s.healthy.unwrap_or(false),
            })
            .collect();
        Self::server_snapshot(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_server(id: &str, model_id: &str, name: &str, port: u16) -> ServerSummary {
        ServerSummary {
            id: id.to_string(),
            model_id: model_id.to_string(),
            model_name: name.to_string(),
            port,
            healthy: Some(true),
        }
    }

    #[test]
    fn test_from_server_started() {
        let server = make_server("srv-1", "42", "test-model", 8080);
        let event = AppEvent::from_server_started(&server);
        match event {
            AppEvent::ServerStarted {
                model_id,
                model_name,
                port,
            } => {
                assert_eq!(model_id, 42);
                assert_eq!(model_name, "test-model");
                assert_eq!(port, 8080);
            }
            _ => panic!("expected ServerStarted"),
        }
    }

    #[test]
    fn test_from_server_stopped() {
        let server = make_server("srv-1", "42", "test-model", 8080);
        let event = AppEvent::from_server_stopped(&server);
        match event {
            AppEvent::ServerStopped {
                model_id,
                model_name,
            } => {
                assert_eq!(model_id, 42);
                assert_eq!(model_name, "test-model");
            }
            _ => panic!("expected ServerStopped"),
        }
    }

    #[test]
    fn test_from_server_error() {
        let server = make_server("srv-1", "42", "test-model", 8080);
        let event = AppEvent::from_server_error(&server, "something failed");
        match event {
            AppEvent::ServerError {
                model_id,
                model_name,
                error,
            } => {
                assert_eq!(model_id, Some(42));
                assert_eq!(model_name, "test-model");
                assert_eq!(error, "something failed");
            }
            _ => panic!("expected ServerError"),
        }
    }

    #[test]
    fn test_from_server_error_invalid_model_id() {
        let server = make_server("srv-1", "abc", "test-model", 8080);
        let event = AppEvent::from_server_error(&server, "something failed");
        match event {
            AppEvent::ServerError {
                model_id,
                model_name,
                error,
            } => {
                assert_eq!(model_id, None);
                assert_eq!(model_name, "test-model");
                assert_eq!(error, "something failed");
            }
            _ => panic!("expected ServerError"),
        }
    }

    #[test]
    fn test_from_server_snapshot() {
        let servers = vec![
            make_server("srv-a", "1", "model-a", 9001),
            make_server("srv-b", "2", "model-b", 9002),
        ];
        let event = AppEvent::from_server_snapshot(&servers);
        match event {
            AppEvent::ServerSnapshot { servers: entries } => {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries[0].model_id, 1);
                assert_eq!(entries[0].port, 9001);
                assert_eq!(entries[1].model_id, 2);
                assert_eq!(entries[1].port, 9002);
            }
            _ => panic!("expected ServerSnapshot"),
        }
    }
}
