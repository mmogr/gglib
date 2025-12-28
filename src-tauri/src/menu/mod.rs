//! Native application menu for GGLib GUI.
//!
//! Provides a cross-platform menu bar with stateful items that reflect
//! the current application state (llama.cpp installation, proxy status,
//! selected model, etc.).
//!
//! # Module Structure
//!
//! - `ids` - Menu item ID constants for event handling
//! - `build` - Menu construction
//! - `handlers` - Menu event handling
//! - `state_sync` - Menu state synchronization with app state

mod build;
pub mod handlers;
pub mod ids;
pub mod state_sync;

pub use build::build_app_menu;

use tauri::{
    menu::MenuItem,
    Wry,
};

/// Holds references to menu items that need dynamic state updates.
///
/// This struct is stored in the application state and used to
/// enable/disable or check/uncheck menu items based on app state.
pub struct AppMenu {
    // Model menu items (enabled based on selection/server state)
    pub start_server: MenuItem<Wry>,
    pub stop_server: MenuItem<Wry>,
    pub remove_model: MenuItem<Wry>,

    // Proxy menu items
    pub start_proxy: MenuItem<Wry>,
    pub stop_proxy: MenuItem<Wry>,
    pub copy_proxy_url: MenuItem<Wry>,

    // Help menu items
    pub install_llama: MenuItem<Wry>,
}

/// State used to synchronize menu item enabled/checked status
#[derive(Debug, Clone, Default)]
pub struct MenuState {
    pub llama_installed: bool,
    pub proxy_running: bool,
    pub model_selected: bool,
    pub selected_model_server_active: bool,
}

impl AppMenu {
    /// Update all stateful menu items based on current application state.
    ///
    /// This should be called whenever relevant state changes:
    /// - Model selection changes
    /// - Server starts/stops
    /// - Proxy starts/stops
    /// - llama.cpp is installed
    pub fn sync_state(&self, state: &MenuState) -> Result<(), tauri::Error> {
        // Model menu items
        // Start Server: enabled if model selected AND not already running
        self.start_server
            .set_enabled(state.model_selected && !state.selected_model_server_active)?;

        // Stop Server: enabled if model selected AND currently running
        self.stop_server
            .set_enabled(state.model_selected && state.selected_model_server_active)?;

        // Remove Model: enabled if model selected
        self.remove_model.set_enabled(state.model_selected)?;

        // Proxy menu items
        // Start/Stop reflect proxy running state
        self.start_proxy.set_enabled(!state.proxy_running)?;
        self.stop_proxy.set_enabled(state.proxy_running)?;
        self.copy_proxy_url.set_enabled(state.proxy_running)?;

        // Help menu items
        // Install llama.cpp: disabled if already installed
        self.install_llama.set_enabled(!state.llama_installed)?;

        Ok(())
    }
}
