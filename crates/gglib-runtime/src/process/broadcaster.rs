//! Server event broadcasting for SSE clients.
//!
//! This module provides infrastructure for broadcasting server lifecycle events
//! to SSE clients in web mode. Desktop (Tauri) mode uses Tauri's event system instead.

use super::events::ServerEvent;
use std::sync::{Arc, LazyLock};
use tokio::sync::broadcast;
use tracing::debug;

/// Broadcast channel capacity for server events
const CHANNEL_CAPACITY: usize = 64;

/// Global server event broadcaster
static EVENT_BROADCASTER: LazyLock<Arc<ServerEventBroadcaster>> =
    LazyLock::new(|| Arc::new(ServerEventBroadcaster::new()));

/// Get the global server event broadcaster
pub fn get_event_broadcaster() -> Arc<ServerEventBroadcaster> {
    EVENT_BROADCASTER.clone()
}

/// Broadcaster for server lifecycle events
pub struct ServerEventBroadcaster {
    sender: broadcast::Sender<ServerEvent>,
}

impl ServerEventBroadcaster {
    /// Create a new broadcaster
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { sender }
    }

    /// Broadcast a server event to all subscribers
    pub fn broadcast(&self, event: ServerEvent) {
        // Only log if there are receivers (avoid spam when no SSE clients)
        if self.sender.receiver_count() > 0 {
            debug!(?event, "Broadcasting server event");
            let _ = self.sender.send(event);
        }
    }

    /// Subscribe to server events
    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.sender.subscribe()
    }

    /// Get number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for ServerEventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}
