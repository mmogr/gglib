//! Centralized event emission for Tauri.
//!
//! This module provides event name constants and emit helpers for all
//! Tauri event types, reducing repetitive error-handling boilerplate.

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::error;

/// Event name constants.
///
/// These match the existing frontend event listeners.
/// Keep strings stable to avoid frontend breakage.
pub mod names {
    // Download events
    pub const DOWNLOAD_PROGRESS: &str = "download-progress";

    // Server lifecycle events
    pub const SERVER_RUNNING: &str = "server:running";
    pub const SERVER_STOPPING: &str = "server:stopping";
    pub const SERVER_STOPPED: &str = "server:stopped";
    pub const SERVER_SNAPSHOT: &str = "server:snapshot";
    pub const SERVER_LOG: &str = "server-log";

    // Llama installation events
    pub const LLAMA_INSTALL_PROGRESS: &str = "llama-install-progress";

    // Menu action events (menu -> frontend)
    pub const MENU_ADD_MODEL_FILE: &str = "menu:add-model-file";
    pub const MENU_SHOW_DOWNLOADS: &str = "menu:show-downloads";
    pub const MENU_REFRESH_MODELS: &str = "menu:refresh-models";
    pub const MENU_START_SERVER: &str = "menu:start-server";
    pub const MENU_STOP_SERVER: &str = "menu:stop-server";
    pub const MENU_REMOVE_MODEL: &str = "menu:remove-model";
    pub const MENU_START_PROXY: &str = "menu:start-proxy";
    pub const MENU_PROXY_STOPPED: &str = "menu:proxy-stopped";
    pub const MENU_PROXY_ERROR: &str = "menu:proxy-error";
    pub const MENU_COPY_TO_CLIPBOARD: &str = "menu:copy-to-clipboard";
    pub const MENU_SHOW_CHAT: &str = "menu:show-chat";
    pub const MENU_TOGGLE_SIDEBAR: &str = "menu:toggle-sidebar";
    pub const MENU_INSTALL_LLAMA: &str = "menu:install-llama";
    pub const MENU_CHECK_LLAMA_STATUS: &str = "menu:check-llama-status";
    pub const MENU_OPEN_SETTINGS: &str = "menu:open-settings";
}

/// Emit an event to the frontend, logging any errors.
///
/// This replaces the repetitive pattern:
/// ```ignore
/// if let Err(e) = app.emit("event", payload) {
///     error!(error = %e, "Failed to emit event");
/// }
/// ```
///
/// With:
/// ```ignore
/// emit_or_log(&app, names::EVENT, payload);
/// ```
pub fn emit_or_log<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(e) = app.emit(event, payload) {
        error!(error = %e, event, "Failed to emit event");
    }
}
