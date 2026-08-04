//! Tray menu event handling.
//!
//! Dispatch only: each arm names one action and hands off. The proxy work
//! lives in [`crate::proxy_actions`] so the tray and the autostart path cannot
//! diverge, and the quit path stays in [`crate::lifecycle`] so the tray gets
//! the same hardened shutdown as every other exit.

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tracing::{debug, error};

use crate::app::AppState;
use crate::app::events::{emit_or_log, names};
use crate::lifecycle;
use crate::proxy_actions;
use crate::tray::placement::Anchor;
use crate::tray::{ids, window};

/// Perform the action a menu item id names.
///
/// The single entry point for every tray backend, so the tray reaches the same
/// code as the WebUI and the CLI however the click arrived.
pub fn dispatch(app: &AppHandle, id: &str) {
    debug!(tray_id = %id, "Tray menu event received");

    match id {
        ids::OPEN_PANEL => {
            spawn_ui(app.clone(), |app| {
                window::toggle_panel(&app, Anchor::Unknown)
            });
        }
        ids::OPEN_MAIN => spawn_ui(app.clone(), |app| window::show_main(&app)),
        ids::START_PROXY => spawn_proxy(app.clone(), true),
        ids::STOP_PROXY => spawn_proxy(app.clone(), false),
        ids::COPY_PROXY_URL => copy_endpoint_url(app),
        ids::PREFERENCES => open_preferences(app),
        ids::QUIT => confirm_quit(app),
        _ => debug!(tray_id = %id, "Unhandled tray menu event"),
    }
}

/// Run a window operation, logging any failure.
fn spawn_ui<F>(app: AppHandle, action: F)
where
    F: FnOnce(AppHandle) -> tauri::Result<()> + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        if let Err(e) = action(app) {
            error!(error = %e, "Tray window action failed");
        }
    });
}

/// Start or stop the proxy off the event thread.
fn spawn_proxy(app: AppHandle, start: bool) {
    tauri::async_runtime::spawn(async move {
        let result = if start {
            proxy_actions::start(&app).await.map(|_| ())
        } else {
            proxy_actions::stop(&app).await
        };

        if let Err(e) = result {
            error!(error = %e, start, "Tray proxy action failed");
        }
    });
}

/// Put the endpoint URL on the clipboard.
///
/// Goes through the frontend because clipboard access is a webview capability
/// here, matching how the application menu does it.
fn copy_endpoint_url(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let port = state.proxy_port.read().await.unwrap_or(8080);
        emit_or_log(
            &app,
            names::MENU_COPY_TO_CLIPBOARD,
            format!("http://127.0.0.1:{port}/v1"),
        );
    });
}

/// Bring the main window forward and ask it to open settings.
///
/// Showing the window first matters: with close-to-tray on, preferences may be
/// chosen while nothing is on screen to open them in.
fn open_preferences(app: &AppHandle) {
    if let Err(e) = window::show_main(app) {
        error!(error = %e, "Failed to show main window for preferences");
    }
    emit_or_log(app, names::MENU_OPEN_SETTINGS, ());
}

/// Confirm before quitting while the proxy is serving, then exit.
///
/// Quitting used to be the only way to close the app, so it needed no
/// warning. With close-to-tray it becomes the one action that takes the
/// endpoint away from whatever is still pointed at it.
fn confirm_quit(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let running = *app.state::<AppState>().proxy_enabled.read().await;

        if running {
            let confirmed = app
                .dialog()
                .message("The proxy is running. Quitting stops it, and any client using the endpoint will lose it.")
                .title("Quit gglib?")
                .kind(MessageDialogKind::Warning)
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "Quit".to_owned(),
                    "Cancel".to_owned(),
                ))
                .blocking_show();

            if !confirmed {
                return;
            }
        }

        // The same entry point Cmd+Q and window close use, so there is exactly
        // one shutdown sequence however the user asked to quit.
        lifecycle::request_shutdown(&app);
    });
}
