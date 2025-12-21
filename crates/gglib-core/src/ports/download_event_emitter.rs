//! Download event emitter port.
//!
//! This port abstracts download event emission, allowing the download manager
//! to emit events without coupling to transport details (SSE, Tauri, etc.).

use std::sync::Arc;

use crate::download::DownloadEvent;
use crate::events::AppEvent;

use super::AppEventEmitter;

/// Port for emitting download events.
///
/// This trait abstracts away the transport mechanism for download events.
/// Implementations handle the actual event delivery (channels, SSE, Tauri events).
///
/// # Example
///
/// ```ignore
/// // In download manager
/// fn on_progress(&self, emitter: &dyn DownloadEventEmitterPort) {
///     emitter.emit(DownloadEvent::DownloadProgress { ... });
/// }
/// ```
pub trait DownloadEventEmitterPort: Send + Sync {
    /// Emit a download event.
    ///
    /// Implementations should handle the event asynchronously or buffer it.
    /// This method should not block.
    fn emit(&self, event: DownloadEvent);

    /// Clone this emitter into a boxed trait object.
    ///
    /// This enables cloning of `Arc<dyn DownloadEventEmitterPort>` without
    /// requiring the underlying type to implement Clone.
    fn clone_box(&self) -> Box<dyn DownloadEventEmitterPort>;
}

/// A no-op download event emitter for tests and CLI contexts.
///
/// This implementation discards all events, making it suitable for:
/// - Unit tests that don't need to verify event emission
/// - CLI applications that handle progress differently
/// - Contexts where event emission is optional
#[derive(Debug, Clone, Default)]
pub struct NoopDownloadEmitter;

impl NoopDownloadEmitter {
    /// Create a new no-op download emitter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl DownloadEventEmitterPort for NoopDownloadEmitter {
    fn emit(&self, _event: DownloadEvent) {
        // Intentionally do nothing
    }

    fn clone_box(&self) -> Box<dyn DownloadEventEmitterPort> {
        Box::new(self.clone())
    }
}

/// Bridge adapter that converts `DownloadEvent` to `AppEvent` and forwards to `AppEventEmitter`.
///
/// This provides a single wiring path: download manager emits `DownloadEvent`,
/// which gets mapped to the appropriate `AppEvent` variant and sent through
/// the shared event infrastructure.
///
/// # Example
///
/// ```ignore
/// let app_emitter: Arc<dyn AppEventEmitter> = /* Tauri or SSE emitter */;
/// let download_emitter = AppEventBridge::new(app_emitter);
///
/// // Pass to download manager
/// let manager = build_download_manager(DownloadManagerDeps {
///     event_emitter: Arc::new(download_emitter),
///     ...
/// });
/// ```
#[derive(Clone)]
pub struct AppEventBridge {
    inner: Arc<dyn AppEventEmitter>,
}

impl AppEventBridge {
    /// Create a new bridge wrapping an `AppEventEmitter`.
    pub fn new(emitter: Arc<dyn AppEventEmitter>) -> Self {
        Self { inner: emitter }
    }

    /// Wrap a `DownloadEvent` in an `AppEvent`.
    ///
    /// Preserves all download event details including shard progress information.
    const fn map_event(event: DownloadEvent) -> AppEvent {
        AppEvent::Download { event }
    }
}

impl DownloadEventEmitterPort for AppEventBridge {
    fn emit(&self, event: DownloadEvent) {
        let app_event = Self::map_event(event);
        self.inner.emit(app_event);
    }

    fn clone_box(&self) -> Box<dyn DownloadEventEmitterPort> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_emitter() {
        let emitter = NoopDownloadEmitter::new();

        // Should not panic
        emitter.emit(DownloadEvent::DownloadStarted {
            id: "test".to_string(),
        });
    }

    #[test]
    fn test_noop_emitter_clone_box() {
        let emitter = NoopDownloadEmitter::new();
        let _boxed: Box<dyn DownloadEventEmitterPort> = emitter.clone_box();
    }

    #[test]
    fn test_arc_emitter() {
        let emitter: Arc<dyn DownloadEventEmitterPort> = Arc::new(NoopDownloadEmitter::new());
        emitter.emit(DownloadEvent::DownloadStarted {
            id: "test".to_string(),
        });
    }

    /// Regression test: `ShardProgress` must be preserved through `AppEventBridge`.
    ///
    /// This prevents the bug where shard detail was collapsed to generic `DownloadProgress`,
    /// causing the UI to lose per-shard and aggregate progress information.
    #[test]
    fn test_shard_progress_preserved_in_bridge() {
        use std::sync::Mutex;

        // Mock AppEventEmitter that captures emitted events
        #[derive(Clone)]
        struct MockEmitter {
            captured: Arc<Mutex<Vec<AppEvent>>>,
        }

        impl AppEventEmitter for MockEmitter {
            fn emit(&self, event: AppEvent) {
                self.captured.lock().unwrap().push(event);
            }

            fn clone_box(&self) -> Box<dyn AppEventEmitter> {
                Box::new(self.clone())
            }
        }

        let captured = Arc::new(Mutex::new(Vec::new()));
        let mock = Arc::new(MockEmitter {
            captured: captured.clone(),
        });
        let bridge = AppEventBridge::new(mock);

        // Emit a ShardProgress event
        let shard_event = DownloadEvent::shard_progress(
            "model:Q4_K_M",
            0,
            4,
            "model-00001-of-00004.gguf",
            500_000,
            1_000_000,
            500_000,
            4_000_000,
            1024.0,
        );

        bridge.emit(shard_event);

        // Verify it's wrapped, not collapsed
        let events = captured.lock().unwrap();
        assert_eq!(events.len(), 1, "Should emit exactly one AppEvent");

        match &events[0] {
            AppEvent::Download { event } => match event {
                DownloadEvent::ShardProgress {
                    shard_index,
                    total_shards,
                    shard_filename,
                    shard_downloaded,
                    shard_total,
                    aggregate_downloaded,
                    aggregate_total,
                    ..
                } => {
                    assert_eq!(*shard_index, 0);
                    assert_eq!(*total_shards, 4);
                    assert_eq!(shard_filename, "model-00001-of-00004.gguf");
                    assert_eq!(*shard_downloaded, 500_000);
                    assert_eq!(*shard_total, 1_000_000);
                    assert_eq!(*aggregate_downloaded, 500_000);
                    assert_eq!(*aggregate_total, 4_000_000);
                }
                _ => panic!("Expected ShardProgress to be preserved, got {event:?}"),
            },
            other => panic!("Expected AppEvent::Download wrapper, got {other:?}"),
        }
    }
}
