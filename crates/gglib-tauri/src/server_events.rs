//! Tauri adapter for server lifecycle events.
//!
//! This module implements the `ServerEvents` port by converting `ServerSummary`
//! to Tauri's `ServerEvent` types and emitting via the Tauri event system.

use gglib_core::events::{AppEvent, ServerEvents, ServerSummary};
use tauri::AppHandle;

use crate::events::emit_or_log;

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
}

// NOTE: This adapter mirrors the Axum ServerEvents implementation (gglib-axum/src/sse.rs).
// All server lifecycle events for Tauri MUST flow through this port implementation.
impl ServerEvents for TauriServerEvents {
    fn started(&self, server: &ServerSummary) {
        let model_id = server.model_id.parse::<i64>().unwrap_or(0);
        let event = AppEvent::server_started(model_id, &server.model_name, server.port);
        emit_or_log(&self.app, event.event_name(), &event);
    }

    fn stopping(&self, server: &ServerSummary) {
        // No canonical AppEvent variant for "stopping".
        tracing::debug!(
            model_id = %server.model_id,
            model_name = %server.model_name,
            "Server stopping"
        );
    }

    fn stopped(&self, server: &ServerSummary) {
        let model_id = server.model_id.parse::<i64>().unwrap_or(0);
        let event = AppEvent::server_stopped(model_id, &server.model_name);
        emit_or_log(&self.app, event.event_name(), &event);
    }

    fn snapshot(&self, servers: &[ServerSummary]) {
        let started_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entries: Vec<gglib_core::events::ServerSnapshotEntry> = servers
            .iter()
            .map(|s| gglib_core::events::ServerSnapshotEntry {
                model_id: s.model_id.parse::<i64>().unwrap_or(0),
                model_name: s.model_name.clone(),
                port: s.port,
                started_at,
                healthy: s.healthy.unwrap_or(false),
            })
            .collect();

        let event = AppEvent::server_snapshot(entries);
        emit_or_log(&self.app, event.event_name(), &event);
    }

    fn error(&self, server: &ServerSummary, error: &str) {
        let model_id = server.model_id.parse::<i64>().ok();
        let event = AppEvent::server_error(model_id, &server.model_name, error);
        emit_or_log(&self.app, event.event_name(), &event);
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
    fn test_app_event_server_started_serialization() {
        let s = summary("42", 8080);
        let model_id = s.model_id.parse::<i64>().unwrap();
        let event = AppEvent::server_started(model_id, &s.model_name, s.port);

        assert_eq!(event.event_name(), "server:started");

        let json = serde_json::to_value(&event).expect("serialization should succeed");
        assert_eq!(json["type"], "server_started");
        assert_eq!(json["modelId"], 42);
        assert_eq!(json["modelName"], "TestModel");
        assert_eq!(json["port"], 8080);
    }

    #[test]
    fn test_app_event_server_stopped_serialization() {
        let s = summary("42", 8080);
        let model_id = s.model_id.parse::<i64>().unwrap();
        let event = AppEvent::server_stopped(model_id, &s.model_name);

        assert_eq!(event.event_name(), "server:stopped");

        let json = serde_json::to_value(&event).expect("serialization should succeed");
        assert_eq!(json["type"], "server_stopped");
        assert_eq!(json["modelId"], 42);
        assert_eq!(json["modelName"], "TestModel");
    }
}
