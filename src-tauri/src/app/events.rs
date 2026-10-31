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
pub(crate) mod names {
    // Menu action events (menu -> frontend). The app menu is macOS-only, so
    // the names only it sends are too; the tray and proxy actions send the
    // other two on every OS.
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_ADD_MODEL_FILE: &str = "menu:add-model-file";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_SHOW_DOWNLOADS: &str = "menu:show-downloads";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_REFRESH_MODELS: &str = "menu:refresh-models";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_START_SERVER: &str = "menu:start-server";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_STOP_SERVER: &str = "menu:stop-server";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_REMOVE_MODEL: &str = "menu:remove-model";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_START_PROXY: &str = "menu:start-proxy";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_PROXY_STOPPED: &str = "menu:proxy-stopped";
    pub(crate) const MENU_COPY_TO_CLIPBOARD: &str = "menu:copy-to-clipboard";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_SHOW_CHAT: &str = "menu:show-chat";
    pub(crate) const MENU_OPEN_SETTINGS: &str = "menu:open-settings";
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
pub(crate) fn emit_or_log<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(e) = app.emit(event, payload) {
        error!(error = %e, event, "Failed to emit event");
    }
}
