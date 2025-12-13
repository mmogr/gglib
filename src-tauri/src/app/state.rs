//! Application state shared across all Tauri commands.

use gglib_core::services::AppCore;
use gglib_mcp::McpService;
use gglib_tauri::gui_backend::GuiBackend;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::menu::AppMenu;

/// Application state with shared backend.
///
/// This struct is managed by Tauri and accessible to all commands
/// via `tauri::State<'_, AppState>`.
pub struct AppState {
    /// Core application facade for direct service access (chat, etc.)
    pub core: Arc<AppCore>,
    /// Shared GUI backend (new architecture from gglib-tauri)
    pub gui: Arc<GuiBackend>,
    /// MCP service for managing MCP servers
    pub mcp: Arc<McpService>,
    /// Port for the embedded API server
    pub api_port: u16,
    /// Menu state for dynamic updates
    pub menu: Arc<RwLock<Option<AppMenu>>>,
    /// Currently selected model ID (for menu state sync)
    pub selected_model_id: Arc<RwLock<Option<i64>>>,
}

impl AppState {
    /// Create a new application state.
    pub fn new(
        core: Arc<AppCore>,
        gui: Arc<GuiBackend>,
        mcp: Arc<McpService>,
        api_port: u16,
    ) -> Self {
        Self {
            core,
            gui,
            mcp,
            api_port,
            menu: Arc::new(RwLock::new(None)),
            selected_model_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Access the core application facade.
    pub fn core(&self) -> &Arc<AppCore> {
        &self.core
    }

    /// Access the MCP service.
    pub fn mcp(&self) -> Arc<McpService> {
        Arc::clone(&self.mcp)
    }

    /// Access the GUI backend.
    pub fn gui(&self) -> &Arc<GuiBackend> {
        &self.gui
    }
}
