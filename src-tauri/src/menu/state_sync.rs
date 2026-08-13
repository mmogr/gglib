//! Menu and tray state synchronization.
//!
//! This module is the single owner of state sync. Everything that changes
//! something a user can see in the application menu or the tray funnels
//! through [`sync_all_state`], so there is one place that decides what the app
//! currently looks like.
//!
//! Two surfaces with different lifetimes: the application menu exists on macOS
//! only, while the tray runs everywhere. Rather than build one combined state
//! object and leave half of it unread on Linux, each surface gathers what it
//! needs — the shared part is just the proxy, which is all the tray displays.

use crate::app::AppState;
use crate::daemon::DaemonSnapshot;
use crate::tray;
use tauri::AppHandle;
use tracing::warn;

/// Sync every surface that displays application state.
///
/// Called by `daemon::watch` whenever the daemon's state changes, and by
/// `commands::util` when the selected model does.
///
/// The snapshot is cloned out of its lock rather than read through it: both
/// surfaces below await, and the tray's await is a D-Bus round trip on Linux.
pub async fn sync_all_state(
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
) -> Result<(), String> {
    let snapshot = state.snapshot.read().await.clone();

    sync_app_menu(state, &snapshot).await?;
    tray::sync(app, snapshot.proxy_running, snapshot.proxy_port).await?;

    Ok(())
}

/// Apply state to the macOS application menu.
///
/// Gathers the model and llama.cpp state the menu needs but the tray does not,
/// so those reads do not happen on platforms with no menu to show them in.
#[cfg(target_os = "macos")]
async fn sync_app_menu(
    state: &tauri::State<'_, AppState>,
    snapshot: &DaemonSnapshot,
) -> Result<(), String> {
    use crate::menu::MenuState;
    use gglib_runtime::llama::check_llama_installed;

    let menu_guard = state.menu.read().await;
    let Some(menu) = menu_guard.as_ref() else {
        // Menu not built yet; the initial sync in setup will cover it.
        return Ok(());
    };

    let selected_id = *state.selected_model_id.read().await;

    menu.sync_state(&MenuState {
        llama_installed: check_llama_installed(),
        proxy_running: snapshot.proxy_running,
        model_selected: selected_id.is_some(),
        // A selected model with a running server enables Stop rather than
        // Start. The snapshot already carries every resident model id, so this
        // no longer costs a `/api/servers` round trip of its own.
        selected_model_server_active: selected_id.is_some_and(|id| snapshot.serves(id)),
    })
    .map_err(|e| format!("Failed to sync menu state: {e}"))
}

/// No application menu exists off macOS — the other platforms are given an
/// empty one at startup precisely so no menu bar appears.
#[cfg(not(target_os = "macos"))]
#[allow(clippy::unused_async)] // Mirrors the macOS signature.
async fn sync_app_menu(
    _state: &tauri::State<'_, AppState>,
    _snapshot: &DaemonSnapshot,
) -> Result<(), String> {
    Ok(())
}

/// Sync every surface, logging rather than returning any failure.
///
/// For fire-and-forget callers with nowhere useful to send an error.
pub async fn sync_all_state_logged(app: &AppHandle, state: &tauri::State<'_, AppState>) {
    if let Err(e) = sync_all_state(app, state).await {
        warn!("Failed to sync menu and tray state: {e}");
    }
}
