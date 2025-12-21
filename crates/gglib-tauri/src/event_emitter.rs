//! Tauri event emitter implementation.
//!
//! This module provides an `AppEventEmitter` implementation that broadcasts
//! events to the Tauri frontend via `AppHandle::emit()`.

use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

/// Tauri-based event emitter that broadcasts `AppEvent` to the frontend.
///
/// Uses the event's `event_name()` method for consistent naming across
/// all adapters, avoiding duplicate string constants.
#[derive(Clone)]
pub struct TauriEventEmitter {
    app_handle: Arc<AppHandle>,
}

impl TauriEventEmitter {
    /// Create a new Tauri event emitter.
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle: Arc::new(app_handle),
        }
    }

    /// Internal helper to emit events with proper error handling.
    fn emit_event(&self, event: &AppEvent) {
        let event_name = event.event_name();
        if let Err(e) = self.app_handle.emit(event_name, event) {
            tracing::warn!(
                event = %event_name,
                error = %e,
                "Failed to emit Tauri event"
            );
        }
    }
}

impl AppEventEmitter for TauriEventEmitter {
    fn emit(&self, event: AppEvent) {
        self.emit_event(&event);
    }

    fn clone_box(&self) -> Box<dyn AppEventEmitter> {
        Box::new(self.clone())
    }
}

impl std::fmt::Debug for TauriEventEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TauriEventEmitter").finish()
    }
}
