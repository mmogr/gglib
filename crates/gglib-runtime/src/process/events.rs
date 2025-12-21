//! Server lifecycle events for real-time state synchronization.
//!
//! These events are emitted by the backend and consumed by the frontend
//! to maintain a synchronized view of server state. The frontend should
//! treat these events as the sole source of truth for server lifecycle.

use serde::{Deserialize, Serialize};

/// Server lifecycle status.
///
/// The status values directly map to event types for consistency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerStatus {
    /// Server is running and accepting requests
    Running,
    /// Server stop has been initiated
    Stopping,
    /// Server has stopped cleanly
    Stopped,
    /// Server crashed or exited unexpectedly
    Crashed,
}

/// A single server's state in snapshot/individual events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStateInfo {
    /// Model ID (string for frontend compatibility)
    pub model_id: String,
    /// Current status
    pub status: ServerStatus,
    /// Port the server is/was running on (when known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Unix timestamp in milliseconds when this state was recorded
    pub updated_at: u64,
}

impl ServerStateInfo {
    /// Create a new `ServerStateInfo` with the current timestamp.
    pub fn new(model_id: u32, status: ServerStatus, port: Option<u16>) -> Self {
        Self {
            model_id: model_id.to_string(),
            status,
            port,
            updated_at: Self::now_ms(),
        }
    }

    /// Get current time as Unix milliseconds.
    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

/// Server lifecycle event payload.
///
/// All server state changes are communicated through this event type.
/// The frontend registry should update its state based on these events,
/// respecting `updated_at` ordering to handle out-of-order delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerEvent {
    /// Snapshot of all currently running servers.
    /// Emitted on app init to seed the frontend registry.
    /// Only contains servers with status=running.
    Snapshot { servers: Vec<ServerStateInfo> },

    /// Server has started and is ready to accept requests.
    Running(ServerStateInfo),

    /// Server stop has been initiated.
    Stopping(ServerStateInfo),

    /// Server has stopped cleanly.
    Stopped(ServerStateInfo),

    /// Server crashed or exited unexpectedly.
    Crashed(ServerStateInfo),
}

impl ServerEvent {
    /// Create a snapshot event from a list of running servers.
    pub fn snapshot(servers: Vec<ServerStateInfo>) -> Self {
        Self::Snapshot { servers }
    }

    /// Create a running event for a server that just started.
    pub fn running(model_id: u32, port: u16) -> Self {
        Self::Running(ServerStateInfo::new(
            model_id,
            ServerStatus::Running,
            Some(port),
        ))
    }

    /// Create a stopping event for a server about to stop.
    pub fn stopping(model_id: u32, port: Option<u16>) -> Self {
        Self::Stopping(ServerStateInfo::new(model_id, ServerStatus::Stopping, port))
    }

    /// Create a stopped event for a server that stopped cleanly.
    pub fn stopped(model_id: u32, port: Option<u16>) -> Self {
        Self::Stopped(ServerStateInfo::new(model_id, ServerStatus::Stopped, port))
    }

    /// Create a crashed event for a server that exited unexpectedly.
    pub fn crashed(model_id: u32, port: Option<u16>) -> Self {
        Self::Crashed(ServerStateInfo::new(model_id, ServerStatus::Crashed, port))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_event_serialization() {
        let event = ServerEvent::running(42, 9000);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"running\""));
        assert!(json.contains("\"modelId\":\"42\""));
        assert!(json.contains("\"port\":9000"));
    }

    #[test]
    fn test_snapshot_serialization() {
        let event = ServerEvent::snapshot(vec![
            ServerStateInfo::new(1, ServerStatus::Running, Some(9000)),
            ServerStateInfo::new(2, ServerStatus::Running, Some(9001)),
        ]);
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"snapshot\""));
        assert!(json.contains("\"servers\""));
    }
}
