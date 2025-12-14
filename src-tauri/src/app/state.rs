//! Application state shared across all Tauri commands.

use gglib_axum::EmbeddedApiInfo;
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
    /// Embedded API server info (port and auth token)
    pub embedded_api: EmbeddedApiInfo,
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
        embedded_api: EmbeddedApiInfo,
    ) -> Self {
        Self {
            core,
            gui,
            mcp,
            embedded_api,
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
