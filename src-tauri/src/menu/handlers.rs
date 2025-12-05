//! Menu event handling.

use crate::app::events::{emit_or_log, names};
use crate::app::AppState;
use crate::menu::{ids, state_sync};
use tauri::{AppHandle, Manager};
use tracing::{debug, error, info};

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
/// Toggles proxy based on current state and syncs menu afterward.
fn handle_proxy_toggle(app: &AppHandle) {
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let state: tauri::State<AppState> = app_clone.state();

        // Check current proxy status
        let proxy_running = match state.backend.get_proxy_status().await {
            Ok(status) => status
                .get("running")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            Err(_) => false,
        };

        if proxy_running {
            // Stop proxy
            if let Err(e) = state.backend.stop_proxy().await {
                error!(error = %e, "Failed to stop proxy from menu");
                emit_or_log(
                    &app_clone,
                    names::MENU_PROXY_ERROR,
                    format!("Failed to stop proxy: {}", e),
                );
            } else {
                info!("Proxy stopped from menu");
                emit_or_log(&app_clone, names::MENU_PROXY_STOPPED, ());
            }
        } else {
            // Start proxy with default settings
            // Frontend should handle this to use proper settings
            emit_or_log(&app_clone, names::MENU_START_PROXY, ());
        }

        // Sync menu state after proxy toggle
        state_sync::sync_menu_state_or_log(&app_clone, &app_clone.state()).await;
    });
}

/// Handle copy proxy URL menu item.
fn handle_copy_proxy_url(app: &AppHandle) {
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let state: tauri::State<AppState> = app_clone.state();

        if let Ok(status) = state.backend.get_proxy_status().await {
            let host = status
                .get("host")
                .and_then(|v| v.as_str())
                .unwrap_or("127.0.0.1");
            let port = status.get("port").and_then(|v| v.as_u64()).unwrap_or(8080);

            let url = format!("http://{}:{}/v1", host, port);
            emit_or_log(&app_clone, names::MENU_COPY_TO_CLIPBOARD, url);
        }
    });
}
