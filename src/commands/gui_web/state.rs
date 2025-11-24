//! Shared application state for the web server.
//!
//! This module defines the state that is shared across all HTTP handlers,
//! using the unified GuiBackend for consistency with the Tauri app.

use crate::services::gui_backend::GuiBackend;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    /// Unified GUI backend service (shared with Tauri)
    pub backend: Arc<GuiBackend>,
    /// Broadcast channel for download progress updates
    pub progress_tx: broadcast::Sender<String>,
}

impl AppState {
    /// Create a new application state with the shared backend
    pub fn new(backend: Arc<GuiBackend>) -> Self {
        let (progress_tx, _) = broadcast::channel(100);
        Self {
            backend,
            progress_tx,
        }
    }
}
