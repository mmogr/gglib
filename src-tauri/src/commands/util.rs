//! Utility commands.
//!
//! These are non-domain-specific commands that don't fit elsewhere.

use crate::app::AppState;
use crate::menu::state_sync;
use gglib_axum::EmbeddedApiInfo;
use tauri::AppHandle;

/// Get embedded API server info (port and auth token).
///
/// The frontend should call this once at startup to discover the embedded
/// server's ephemeral port and authentication token for API requests.
#[tauri::command]
pub fn get_embedded_api_info(state: tauri::State<'_, AppState>) -> EmbeddedApiInfo {
    state.embedded_api.clone()
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
    state_sync::sync_menu_state_internal(&app, &state).await
}

/// Sync menu state based on current application state.
#[tauri::command]
pub async fn sync_menu_state(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state_sync::sync_menu_state_internal(&app, &state).await
}

/// Update proxy running state and sync menu.
///
/// Called by frontend when proxy is started or stopped to keep menu in sync.
#[tauri::command]
pub async fn set_proxy_state(
    running: bool,
    port: Option<u16>,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // Update proxy state
    *state.proxy_enabled.write().await = running;
    *state.proxy_port.write().await = port;

    // Sync menu state
    state_sync::sync_menu_state_internal(&app, &state).await
}
