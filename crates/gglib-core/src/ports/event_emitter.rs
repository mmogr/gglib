//! Event emitter trait for cross-crate event broadcasting.
//!
//! This module defines the abstraction for emitting application events.
//! Implementations handle transport details (channels, Tauri events, SSE, etc.).

use crate::events::AppEvent;

/// Trait for emitting application events.
///
/// This abstraction keeps event plumbing consistent across domains and prevents
/// channel types from becoming part of the public API surface.
///
/// # Implementations
///
/// - `NoopEmitter` - For tests and CLI contexts that don't need events
/// - Adapter-specific implementations (Tauri, Axum SSE, etc.)
///
/// # Example
///
/// ```ignore
/// // In a service
/// fn start_server(&self, emitter: Arc<dyn AppEventEmitter>) {
///     // ... start server logic ...
///     emitter.emit(AppEvent::McpServerStarted { ... });
/// }
/// ```
pub trait AppEventEmitter: Send + Sync {
    /// Emit an application event.
    ///
    /// Implementations should handle the event asynchronously or buffer it.
    /// This method should not block.
    fn emit(&self, event: AppEvent);

    /// Clone this emitter into a boxed trait object.
    ///
    /// This enables cloning of `Arc<dyn AppEventEmitter>` without requiring
    /// the underlying type to implement Clone.
    fn clone_box(&self) -> Box<dyn AppEventEmitter>;
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

    fn clone_box(&self) -> Box<dyn AppEventEmitter> {
        Box::new(self.clone())
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
    fn test_noop_emitter_clone_box() {
        let emitter = NoopEmitter::new();
        let _boxed: Box<dyn AppEventEmitter> = emitter.clone_box();
    }

    #[test]
    fn test_arc_emitter() {
        let emitter: Arc<dyn AppEventEmitter> = Arc::new(NoopEmitter::new());
        emitter.emit(AppEvent::model_removed(1));
    }
}
