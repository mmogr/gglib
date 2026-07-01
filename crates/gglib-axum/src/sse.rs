//! SSE event broadcaster for real-time event streaming.
//!
//! This module provides an SSE broadcaster that implements the core event
//! emitter ports, allowing the download manager and MCP service to emit
//! events that are streamed to connected web clients.
//!
//! The actual broadcast-channel + SSE-encoding plumbing lives in the shared
//! `gglib-sse` crate (a dependency-free leaf); this module just wraps it to
//! implement the `AppEventEmitter` port, keeping that port-implementation
//! glue in the adapter layer where it belongs.

use std::convert::Infallible;
use std::sync::Arc;

use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use gglib_core::events::{AppEvent, ServerEvents, ServerSummary};
use gglib_core::ports::AppEventEmitter;
use gglib_sse::{Broadcaster, SseOptions};

/// SSE broadcaster that implements event emitter ports.
///
/// Events are sent via a broadcast channel and streamed to connected clients.
/// Multiple clients can receive the same events simultaneously.
#[derive(Clone)]
pub struct SseBroadcaster {
    inner: Arc<Broadcaster<AppEvent>>,
}

impl std::fmt::Debug for SseBroadcaster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SseBroadcaster")
            .field("subscriber_count", &self.subscriber_count())
            .finish()
    }
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
        Self {
            inner: Arc::new(Broadcaster::new(capacity)),
        }
    }

    /// Create a new SSE broadcaster with default capacity (256 events).
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(256)
    }

    /// Create an SSE stream for a new client connection.
    ///
    /// Returns an Axum SSE response that streams events to the client.
    /// Includes a keep-alive ping every 30 seconds to prevent proxy timeouts.
    pub fn subscribe(
        &self,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static + use<>> {
        self.inner.clone().subscribe(SseOptions::default())
    }

    /// Get the number of active subscribers.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.inner.subscriber_count()
    }
}

impl AppEventEmitter for SseBroadcaster {
    fn emit(&self, event: AppEvent) {
        self.inner.send(event);
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
        use tokio_stream::StreamExt as _;

        let broadcaster = SseBroadcaster::with_defaults();
        let mut stream = broadcaster.inner.subscribe_events();

        AppEventEmitter::emit(&broadcaster, AppEvent::model_removed(42));

        let event = stream.next().await.unwrap();
        match event {
            AppEvent::ModelRemoved { model_id } => assert_eq!(model_id, 42),
            _ => panic!("Unexpected event type"),
        }
    }
}
