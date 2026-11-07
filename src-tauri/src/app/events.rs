//! Centralized event emission for Tauri.
//!
//! This module provides event name constants and emit helpers for all
//! Tauri event types, reducing repetitive error-handling boilerplate.

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::error;

/// Event name constants.
///
/// `src/services/platform/menuEvents.ts` holds the same names, and
/// `tests/ts/contracts/menuEvents.test.ts` reads this block to keep the two
/// equal: a name on one side only is a menu item that does nothing.
pub(crate) mod names {
    // Menu action events (menu -> frontend). The app menu is macOS-only, so
    // the names only it sends are too; the tray sends the other two on every
    // OS (its Settings… navigates, its Copy Endpoint URL copies).
    /// Go to a place; the payload is one of [`super::places`].
    pub(crate) const MENU_NAVIGATE: &str = "menu:navigate";
    pub(crate) const MENU_COPY_TO_CLIPBOARD: &str = "menu:copy-to-clipboard";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_HIDE_INSPECTOR: &str = "menu:hide-inspector";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_LOAD_MODEL: &str = "menu:load-model";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_LOAD_WITH_OPTIONS: &str = "menu:load-with-options";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_UNLOAD_MODEL: &str = "menu:unload-model";
    #[cfg(target_os = "macos")]
    pub(crate) const MENU_REMOVE_MODEL: &str = "menu:remove-model";
}

/// The places [`names::MENU_NAVIGATE`] can name, its payload.
///
/// The frontend turns each into an address (`src/app/menuBridge.ts`); the
/// same contract test holds its table to this block. Settings… is on the
/// tray menu too, so it is the one place every OS sends.
pub(crate) mod places {
    pub(crate) const SETTINGS: &str = "settings";
    #[cfg(target_os = "macos")]
    pub(crate) const LIBRARY: &str = "library";
    #[cfg(target_os = "macos")]
    pub(crate) const ENDPOINT: &str = "endpoint";
    #[cfg(target_os = "macos")]
    pub(crate) const DEVICES: &str = "devices";
    /// The latest chat, else an empty one.
    #[cfg(target_os = "macos")]
    pub(crate) const CHAT: &str = "chat";
    #[cfg(target_os = "macos")]
    pub(crate) const NEW_CHAT: &str = "new-chat";
    #[cfg(target_os = "macos")]
    pub(crate) const DIAGNOSTICS: &str = "diagnostics";
    /// The library's search, for Hugging Face.
    #[cfg(target_os = "macos")]
    pub(crate) const GET_MODELS: &str = "get-models";
    /// The library, adding a GGUF file.
    #[cfg(target_os = "macos")]
    pub(crate) const ADD_FILE: &str = "add-file";
    /// The endpoint place's Connect a client block.
    #[cfg(target_os = "macos")]
    pub(crate) const CONNECT_CLIENT: &str = "connect-client";
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
