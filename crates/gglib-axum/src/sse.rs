//! SSE event broadcaster for real-time event streaming.
//!
//! This module provides an SSE broadcaster that implements the core event
//! emitter ports, allowing the download manager and MCP service to emit
//! events that are streamed to connected web clients.

use std::convert::Infallible;
use std::sync::Arc;

use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use gglib_core::events::{AppEvent, ServerEvents, ServerSummary};
use gglib_core::ports::AppEventEmitter;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

/// SSE broadcaster that implements event emitter ports.
///
/// Events are sent via a broadcast channel and streamed to connected clients.
/// Multiple clients can receive the same events simultaneously.
#[derive(Debug, Clone)]
pub struct SseBroadcaster {
    sender: broadcast::Sender<AppEvent>,
}

impl SseBroadcaster {
    /// Create a new SSE broadcaster with the specified channel capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of events that can be buffered.
    ///   Slow clients may miss events if the buffer overflows.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Create a new SSE broadcaster with default capacity (256 events).
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(256)
    }

    /// Create an SSE stream for a new client connection.
    ///
    /// Returns an Axum SSE response that streams events to the client.
    /// Takes `Arc<Self>` to ensure proper ownership for the stream.
    /// Includes a keep-alive ping every 30 seconds to prevent proxy timeouts.
    pub fn subscribe(
        self: Arc<Self>,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
        let receiver = self.sender.subscribe();
        let stream = BroadcastStream::new(receiver).filter_map(|result| {
            match result {
                Ok(event) => {
                    // Serialize event to JSON
                    match serde_json::to_string(&event) {
                        Ok(json) => Some(Ok(Event::default().data(json))),
                        Err(e) => {
                            tracing::warn!("Failed to serialize event: {}", e);
                            None
                        }
                    }
                }
                Err(e) => {
                    // Log lagged or closed errors and continue
                    tracing::debug!("SSE stream error: {}", e);
                    None
                }
            }
        });

        Sse::new(stream).keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(std::time::Duration::from_secs(30))
                .text("ping"),
        )
    }

    /// Get the number of active subscribers.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl AppEventEmitter for SseBroadcaster {
    fn emit(&self, event: AppEvent) {
        // Send event to all subscribers
        // Ignore send errors (no subscribers is fine)
        let _ = self.sender.send(event);
    }

    fn clone_box(&self) -> Box<dyn AppEventEmitter> {
        Box::new(self.clone())
    }
}

/// Create a shared SSE broadcaster wrapped in Arc.
#[must_use]
pub fn create_broadcaster() -> Arc<SseBroadcaster> {
    Arc::new(SseBroadcaster::with_defaults())
}

/// Axum adapter for server lifecycle events via SSE.
///
/// This adapter implements the `ServerEvents` trait by converting
/// `ServerSummary` instances to `AppEvent` variants and emitting them
/// via the SSE broadcaster. This keeps the core lifecycle logic
/// transport-agnostic while allowing Axum-specific event delivery.
#[derive(Debug, Clone)]
pub struct AxumServerEvents {
    broadcaster: SseBroadcaster,
}

impl AxumServerEvents {
    /// Create a new Axum server events adapter.
    #[must_use]
    pub fn new(broadcaster: SseBroadcaster) -> Self {
        Self { broadcaster }
    }
}

impl ServerEvents for AxumServerEvents {
    fn started(&self, server: &ServerSummary) {
        let model_id = server.model_id.parse::<i64>().unwrap_or(0);
        let event = AppEvent::server_started(model_id, &server.model_name, server.port);
        self.broadcaster.emit(event);
    }

    fn stopping(&self, server: &ServerSummary) {
        // Note: There's no AppEvent::ServerStopping variant currently
        // We could add one or just emit stopped after the fact
        tracing::debug!(
            model_id = %server.model_id,
            model_name = %server.model_name,
            "Server stopping"
        );
    }

    fn stopped(&self, server: &ServerSummary) {
        let model_id = server.model_id.parse::<i64>().unwrap_or(0);
        let event = AppEvent::server_stopped(model_id, &server.model_name);
        self.broadcaster.emit(event);
    }

    fn snapshot(&self, servers: &[ServerSummary]) {
        let entries: Vec<gglib_core::events::ServerSnapshotEntry> = servers
            .iter()
            .map(|s| gglib_core::events::ServerSnapshotEntry {
                model_id: s.model_id.parse::<i64>().unwrap_or(0),
                model_name: s.model_name.clone(),
                port: s.port,
                started_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                healthy: s.healthy.unwrap_or(false),
            })
            .collect();

        let event = AppEvent::server_snapshot(entries);
        self.broadcaster.emit(event);
    }

    fn error(&self, server: &ServerSummary, error: &str) {
        let model_id = server.model_id.parse::<i64>().ok();
        let event = AppEvent::server_error(model_id, &server.model_name, error);
        self.broadcaster.emit(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_broadcaster_creation() {
        let broadcaster = SseBroadcaster::with_defaults();
        assert_eq!(broadcaster.subscriber_count(), 0);
    }

    #[test]
    fn test_emit_without_subscribers() {
        let broadcaster = SseBroadcaster::with_defaults();
        // Should not panic even with no subscribers
        AppEventEmitter::emit(&broadcaster, AppEvent::model_removed(1));
    }

    #[tokio::test]
    async fn test_subscriber_receives_events() {
        let broadcaster = SseBroadcaster::with_defaults();
        let mut receiver = broadcaster.sender.subscribe();

        AppEventEmitter::emit(&broadcaster, AppEvent::model_removed(42));

        let event = receiver.recv().await.unwrap();
        match event {
            AppEvent::ModelRemoved { model_id } => assert_eq!(model_id, 42),
            _ => panic!("Unexpected event type"),
        }
    }
}
