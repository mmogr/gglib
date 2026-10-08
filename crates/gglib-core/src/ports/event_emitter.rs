//! Event emitter trait for cross-crate event broadcasting.
//!
//! This module defines the abstraction for emitting application events.
//! Implementations handle the transport: the daemon's sends each event to its
//! SSE subscribers.

use crate::events::AppEvent;

/// Trait for emitting application events.
///
/// This abstraction keeps event plumbing consistent across domains and prevents
/// channel types from becoming part of the public API surface.
///
/// # Implementations
///
/// - `NoopEmitter` - For tests, and wherever nobody is listening
/// - `SseBroadcaster` in `gglib-axum` - The daemon's, which sends each event
///   to its SSE subscribers
/// - `LibraryChanges` in `gglib-cli` - A command's, which keeps what
///   `ModelOps` emits and posts it to the daemon
///
/// # Example
///
/// ```ignore
/// // In a service
/// fn start_server(&self, emitter: Arc<dyn AppEventEmitter>) {
///     // ... start server logic ...
///     emitter.emit(AppEvent::server_started(model_id, model_name, port));
/// }
/// ```
pub trait AppEventEmitter: Send + Sync {
    /// Emit an application event.
    ///
    /// Implementations should handle the event asynchronously or buffer it.
    /// This method should not block.
    fn emit(&self, event: AppEvent);
}

/// A no-op event emitter for tests and CLI contexts.
///
/// This implementation discards all events, making it suitable for:
/// - Unit tests that don't need to verify event emission
/// - CLI applications that don't have an event listener
/// - Contexts where event emission is optional
#[derive(Debug, Clone, Default)]
pub struct NoopEmitter;

impl NoopEmitter {
    /// Create a new no-op emitter.
    pub const fn new() -> Self {
        Self
    }
}

impl AppEventEmitter for NoopEmitter {
    fn emit(&self, _event: AppEvent) {
        // Intentionally do nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_noop_emitter() {
        let emitter = NoopEmitter::new();

        // Should not panic
        emitter.emit(AppEvent::model_removed(1));
    }

    #[test]
    fn test_arc_emitter() {
        let emitter: Arc<dyn AppEventEmitter> = Arc::new(NoopEmitter::new());
        emitter.emit(AppEvent::model_removed(1));
    }
}
