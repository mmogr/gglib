//! Utility commands.
//!
//! These are non-domain-specific commands that don't fit elsewhere.

use crate::app::AppState;
use crate::menu::state_sync;
use tauri::AppHandle;

/// Where the frontend reaches the backend API.
///
/// The name and shape predate the daemon: the WebView used to talk to an
/// embedded server on an ephemeral port with a bearer token. It now points
/// at the gglib daemon's fixed loopback port; the empty token means "send no
/// Authorization header".
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApiInfo {
    /// Port of the daemon's management API.
    pub port: u16,
    /// Always empty — the daemon's loopback API is unauthenticated.
    pub token: String,
}

/// Get backend API info (port and auth token).
///
/// The frontend calls this once at startup to discover where the API lives.
#[tauri::command]
pub fn get_embedded_api_info() -> ApiInfo {
    ApiInfo {
        port: gglib_core::DAEMON_PORT,
        token: String::new(),
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

/// Sync menu state based on current application state.
#[tauri::command]
pub async fn sync_menu_state(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state_sync::sync_all_state(&app, &state).await
}
