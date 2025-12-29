//! Menu state synchronization.
//!
//! This module is the single owner of menu sync logic.
//! All menu state updates flow through `sync_menu_state_internal`.

use crate::app::AppState;
use crate::menu::MenuState;
use gglib_runtime::llama::check_llama_installed;
use tauri::AppHandle;
#[cfg(target_os = "macos")]
use tracing::warn;

/// Sync menu state based on current application state.
///
/// This is the single source of truth for menu state synchronization.
/// Called by:
/// - `commands::util::sync_menu_state` (Tauri command)
/// - `commands::util::set_selected_model` (after model selection)
/// - Menu handlers (after proxy toggle)
/// - App setup (initial sync)
pub async fn sync_menu_state_internal(
    _app: &AppHandle,
    state: &tauri::State<'_, AppState>,
) -> Result<(), String> {
    let menu_guard = state.menu.read().await;
    let Some(menu) = menu_guard.as_ref() else {
        // Menu not yet initialized, skip
        return Ok(());
    };

    // Gather current state
    let llama_installed = check_llama_installed();

    // Get real proxy state from app state
    let proxy_running = *state.proxy_enabled.read().await;

    let selected_id = *state.selected_model_id.read().await;
    let model_selected = selected_id.is_some();

    // Check if selected model has a running server
    let selected_model_server_active = if let Some(id) = selected_id {
        let servers = state.gui.list_servers().await;
        servers.iter().any(|s| s.model_id == id)
    } else {
        false
    };

    let menu_state = MenuState {
        llama_installed,
        proxy_running,
        model_selected,
        selected_model_server_active,
    };

    // Update menu items
    menu.sync_state(&menu_state)
        .map_err(|e| format!("Failed to sync menu state: {}", e))?;

    Ok(())
}

/// Sync menu state, logging any errors.
///
/// Convenience wrapper for fire-and-forget sync from async contexts.
#[cfg(target_os = "macos")]
pub async fn sync_menu_state_logged(app: &AppHandle, state: &tauri::State<'_, AppState>) {
    if let Err(e) = sync_menu_state_internal(app, state).await {
        warn!("Failed to sync menu state: {}", e);
    }
}

/// Alias for sync_menu_state_logged (backward compatibility).
#[cfg(target_os = "macos")]
pub async fn sync_menu_state_or_log(app: &AppHandle, state: &tauri::State<'_, AppState>) {
    sync_menu_state_logged(app, state).await;
}
