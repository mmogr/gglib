//! Utility commands.
//!
//! These are non-domain-specific commands that don't fit elsewhere.

use crate::app::AppState;
use crate::menu::state_sync;
use tauri::AppHandle;

/// Where the frontend reaches the backend API.
///
/// The name predates the daemon: the WebView used to talk to an embedded
/// server on an ephemeral port with a bearer token. It now points at the
/// daemon's fixed loopback port, which is unauthenticated — the token field
/// went with the embedded server, having been an empty string ever since.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiInfo {
    /// Port of the daemon's management API.
    pub port: u16,
}

/// Get backend API info (port and auth token).
///
/// The frontend calls this once at startup to discover where the API lives.
#[tauri::command]
pub fn get_embedded_api_info() -> ApiInfo {
    ApiInfo {
        port: gglib_core::DAEMON_PORT,
    }
}

/// Open a URL in the system's default browser.
///
/// Used by the frontend to open external links (e.g., HuggingFace model pages).
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| format!("Failed to open URL: {}", e))
}

/// Set the currently selected model ID and sync menu state.
///
/// The selection lives in this process, not in the daemon, so repainting from
/// the snapshot already in hand is correct here — nothing about the daemon
/// changed.
#[tauri::command]
pub async fn set_selected_model(
    model_id: Option<i64>,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // Update selected model ID
    *state.selected_model_id.write().await = model_id;

    // Sync menu state
    state_sync::sync_all_state(&app, &state).await
}

/// Sync menu state after the frontend has changed something.
///
/// Every caller is a fire-and-forget `syncMenuStateSilent()` after an action
/// that changed something the menu shows: a server stopped, a model removed,
/// llama.cpp installed. Two kinds of state are involved and they need
/// different treatment, which is why this does both things.
///
/// **Ask for a poll**, because `sync_all_state` paints from
/// `AppState::snapshot` and only `daemon::watch` writes it. Painting without
/// asking redraws from what was true *before* the action, and it stays wrong
/// until the next tick. The Rust-side callers were all converted to
/// `Refresh::now` when the watcher landed; this is the frontend's equivalent
/// and was missed.
///
/// **Then paint anyway**, because not everything the menu reads is in that
/// snapshot — `llama_installed` is a filesystem check, and an install changes
/// it without changing anything the daemon would report. The watcher would see
/// no difference and skip the repaint.
///
/// So: the immediate paint catches the local state, and the poll it just asked
/// for catches the daemon's, a moment later.
#[tauri::command]
pub async fn sync_menu_state(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.refresh.now();
    state_sync::sync_all_state(&app, &state).await
}
