//! Tauri adapter for server lifecycle events.
//!
//! This module implements the `ServerEvents` port by converting `ServerSummary`
//! to Tauri's `ServerEvent` types and emitting via the Tauri event system.

use gglib_core::events::{ServerEvents, ServerSummary};
use gglib_runtime::process::{ServerEvent, ServerStateInfo, ServerStatus};
use tauri::AppHandle;

use crate::events::{emit_or_log, names};

/// Tauri adapter for server lifecycle events.
///
/// Implements the `ServerEvents` port by converting `ServerSummary` to Tauri's
/// `ServerEvent` and emitting via Tauri's event system. This ensures consistent
/// event emission across the application, matching the Axum SSE adapter pattern.
#[derive(Clone)]
pub struct TauriServerEvents {
    app: AppHandle,
}

impl TauriServerEvents {
    /// Create a new TauriServerEvents adapter.
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    /// Parse model_id with logging on failure.
    fn parse_model_id(server: &ServerSummary) -> u32 {
        server.parsed_model_id().unwrap_or_else(|| {
            tracing::warn!(
                model_id = %server.model_id,
                "Failed to parse ServerSummary.model_id; defaulting to 0"
            );
            0
        })
    }
}

// NOTE: This adapter mirrors the Axum ServerEvents implementation (gglib-axum/src/sse.rs).
// All server lifecycle events for Tauri MUST flow through this port implementation.
impl ServerEvents for TauriServerEvents {
    fn started(&self, server: &ServerSummary) {
        let model_id = Self::parse_model_id(server);
        let event = ServerEvent::running(model_id, server.port);
        emit_or_log(&self.app, names::SERVER_RUNNING, &event);
    }

    fn stopping(&self, server: &ServerSummary) {
        let model_id = Self::parse_model_id(server);
        let state = ServerStateInfo::new(model_id, ServerStatus::Stopping, Some(server.port));
        let event = ServerEvent::Stopping(state);
        emit_or_log(&self.app, names::SERVER_STOPPING, &event);
    }

    fn stopped(&self, server: &ServerSummary) {
        let model_id = Self::parse_model_id(server);
        let state = ServerStateInfo::new(model_id, ServerStatus::Stopped, Some(server.port));
        let event = ServerEvent::Stopped(state);
        emit_or_log(&self.app, names::SERVER_STOPPED, &event);
    }

    fn snapshot(&self, servers: &[ServerSummary]) {
        let states: Vec<ServerStateInfo> = servers
            .iter()
            .map(|s| {
                let model_id = Self::parse_model_id(s);
                ServerStateInfo::new(model_id, ServerStatus::Running, Some(s.port))
            })
            .collect();

        let event = ServerEvent::snapshot(states);
        emit_or_log(&self.app, names::SERVER_SNAPSHOT, &event);
    }

    fn error(&self, server: &ServerSummary, error: &str) {
        // NOTE: Tauri did not previously emit SERVER_ERROR events.
        // Keep as logging-only to maintain behavior-neutral refactor.
        // If we introduce SERVER_ERROR events, do it in a separate issue.
        tracing::error!(
            model_id = %server.model_id,
            model_name = %server.model_name,
            error = %error,
            "Server error"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a ServerSummary for testing.
    fn summary(model_id: &str, port: u16) -> ServerSummary {
        ServerSummary {
            id: format!("test-{}", model_id),
            model_id: model_id.to_string(),
            model_name: "TestModel".to_string(),
            port,
            healthy: Some(true),
        }
    }

    #[test]
    fn test_parse_model_id_valid() {
        let summary = summary("42", 8080);
        assert_eq!(summary.parsed_model_id(), Some(42));
    }

    #[test]
    fn test_parse_model_id_invalid() {
        let summary = summary("not-a-number", 8080);
        assert_eq!(summary.parsed_model_id(), None);
    }

    #[test]
    fn test_server_event_running_serialization() {
        // Verify that ServerEvent::running serializes to expected JSON shape
        let event = ServerEvent::running(42, 8080);
        let json = serde_json::to_value(&event).expect("serialization should succeed");

        assert_eq!(json["type"], "running");
        assert_eq!(json["modelId"], "42"); // model_id is serialized as string
        assert_eq!(json["status"], "running");
        assert_eq!(json["port"], 8080);
        assert!(json["updatedAt"].is_number());
    }

    #[test]
    fn test_server_event_stopping_serialization() {
        let state = ServerStateInfo::new(42, ServerStatus::Stopping, Some(8080));
        let event = ServerEvent::Stopping(state);
        let json = serde_json::to_value(&event).expect("serialization should succeed");

        assert_eq!(json["type"], "stopping");
        assert_eq!(json["modelId"], "42");
        assert_eq!(json["status"], "stopping");
        assert_eq!(json["port"], 8080);
    }

    #[test]
    fn test_server_event_stopped_serialization() {
        let state = ServerStateInfo::new(42, ServerStatus::Stopped, Some(8080));
        let event = ServerEvent::Stopped(state);
        let json = serde_json::to_value(&event).expect("serialization should succeed");

        assert_eq!(json["type"], "stopped");
        assert_eq!(json["modelId"], "42");
        assert_eq!(json["status"], "stopped");
        assert_eq!(json["port"], 8080);
    }

    #[test]
    fn test_snapshot_event_serialization() {
        let states = vec![
            ServerStateInfo::new(1, ServerStatus::Running, Some(8080)),
            ServerStateInfo::new(2, ServerStatus::Running, Some(8081)),
        ];
        let event = ServerEvent::snapshot(states);
        let json = serde_json::to_value(&event).expect("serialization should succeed");

        assert_eq!(json["type"], "snapshot");
        assert!(json["servers"].is_array());
        let servers_array = json["servers"].as_array().unwrap();
        assert_eq!(servers_array.len(), 2);

        // Verify first server in snapshot
        assert_eq!(servers_array[0]["modelId"], "1");
        assert_eq!(servers_array[0]["status"], "running");
        assert_eq!(servers_array[0]["port"], 8080);
    }

    #[test]
    fn test_snapshot_with_empty_servers() {
        let event = ServerEvent::snapshot(vec![]);
        let json = serde_json::to_value(&event).expect("serialization should succeed");

        assert_eq!(json["type"], "snapshot");
        assert_eq!(json["servers"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_parse_model_id_helper() {
        let s1 = summary("123", 8080);
        assert_eq!(TauriServerEvents::parse_model_id(&s1), 123);

        let s2 = summary("invalid", 8080);
        // Should default to 0 (with warning logged)
        assert_eq!(TauriServerEvents::parse_model_id(&s2), 0);
    }
}
