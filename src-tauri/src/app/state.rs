//! Application state shared across all Tauri commands.

use gglib::services::gui_backend::GuiBackend;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::menu::AppMenu;

/// Application state with shared backend.
///
/// This struct is managed by Tauri and accessible to all commands
/// via `tauri::State<'_, AppState>`.
pub struct AppState {
    /// Shared GUI backend (same as Web GUI)
    pub backend: Arc<GuiBackend>,
    /// Port for the embedded API server
    pub api_port: u16,
    /// Menu state for dynamic updates
    pub menu: Arc<RwLock<Option<AppMenu>>>,
    /// Currently selected model ID (for menu state sync)
    pub selected_model_id: Arc<RwLock<Option<u32>>>,
}

impl AppState {
    /// Create a new application state.
    pub fn new(
        backend: Arc<GuiBackend>,
        api_port: u16,
    ) -> Self {
        Self {
            backend,
            api_port,
            menu: Arc::new(RwLock::new(None)),
            selected_model_id: Arc::new(RwLock::new(None)),
        }
    }
}
