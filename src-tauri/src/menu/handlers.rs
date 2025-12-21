//! Menu event handling.

use crate::app::events::{emit_or_log, names};
use crate::app::AppState;
use crate::menu::{ids, state_sync};
use tauri::{AppHandle, Manager};
use tracing::{debug, warn};

/// Handle menu item click events.
pub fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref();

    debug!(menu_id = %id, "Menu event received");

    match id {
        // File menu
        ids::ADD_MODEL_FILE => {
            emit_or_log(app, names::MENU_ADD_MODEL_FILE, ());
        }
        ids::DOWNLOAD_MODEL => {
            emit_or_log(app, names::MENU_SHOW_DOWNLOADS, ());
        }
        ids::REFRESH_MODELS => {
            emit_or_log(app, names::MENU_REFRESH_MODELS, ());
        }

        // Model menu
        ids::START_SERVER => {
            emit_or_log(app, names::MENU_START_SERVER, ());
        }
        ids::STOP_SERVER => {
            emit_or_log(app, names::MENU_STOP_SERVER, ());
        }
        ids::REMOVE_MODEL => {
            emit_or_log(app, names::MENU_REMOVE_MODEL, ());
        }

        // Proxy menu
        ids::PROXY_TOGGLE => {
            handle_proxy_toggle(app);
        }
        ids::COPY_PROXY_URL => {
            handle_copy_proxy_url(app);
        }

        // View menu
        ids::SHOW_DOWNLOADS => {
            emit_or_log(app, names::MENU_SHOW_DOWNLOADS, ());
        }
        ids::SHOW_CHAT => {
            emit_or_log(app, names::MENU_SHOW_CHAT, ());
        }
        ids::TOGGLE_SIDEBAR => {
            emit_or_log(app, names::MENU_TOGGLE_SIDEBAR, ());
        }

        // Help menu
        ids::INSTALL_LLAMA => {
            emit_or_log(app, names::MENU_INSTALL_LLAMA, ());
        }
        ids::CHECK_LLAMA_STATUS => {
            emit_or_log(app, names::MENU_CHECK_LLAMA_STATUS, ());
        }
        ids::OPEN_DOCS => {
            // Open documentation URL
            let _ = open::that("https://github.com/mmogr/gglib");
        }

        // App menu
        ids::PREFERENCES => {
            emit_or_log(app, names::MENU_OPEN_SETTINGS, ());
        }

        _ => {
            debug!(menu_id = %id, "Unhandled menu event");
        }
    }
}

/// Handle proxy toggle menu item.
///
/// NOTE: Proxy not yet integrated with Tauri menu.
fn handle_proxy_toggle(app: &AppHandle) {
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        // Proxy integration pending
        warn!("Proxy toggle requested but proxy not yet integrated with menu");
        emit_or_log(
            &app_clone,
            names::MENU_PROXY_ERROR,
            "Proxy is temporarily disabled during refactor".to_string(),
        );

        // Sync menu state
        let _state: tauri::State<AppState> = app_clone.state();
        state_sync::sync_menu_state_or_log(&app_clone, &app_clone.state()).await;
    });
}

/// Handle copy proxy URL menu item.
fn handle_copy_proxy_url(app: &AppHandle) {
    // Proxy is disabled during Phase 2 refactor
    // Just emit a default URL format
    let url = "http://127.0.0.1:8080/v1".to_string();
    emit_or_log(app, names::MENU_COPY_TO_CLIPBOARD, url);
}
