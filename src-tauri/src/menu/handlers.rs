//! Menu event handling.

use crate::app::AppState;
use crate::app::events::{emit_or_log, names};
use crate::menu::ids;
use tauri::{AppHandle, Manager};
use tracing::debug;

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
        ids::START_PROXY => {
            emit_or_log(app, names::MENU_START_PROXY, ());
        }
        ids::STOP_PROXY => {
            emit_or_log(app, names::MENU_PROXY_STOPPED, ());
        }
        ids::COPY_PROXY_URL => {
            handle_copy_proxy_url(app);
        }

        // View menu
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

/// Handle copy proxy URL menu item.
fn handle_copy_proxy_url(app: &AppHandle) {
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let state: tauri::State<AppState> = app_clone.state();
        let proxy_port = *state.proxy_port.read().await;

        // Build URL from stored proxy port (or default)
        let port = proxy_port.unwrap_or(8080);
        let url = format!("http://127.0.0.1:{}/v1", port);
        emit_or_log(&app_clone, names::MENU_COPY_TO_CLIPBOARD, url);
    });
}
