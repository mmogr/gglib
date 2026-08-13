//! Tray menu event handling.
//!
//! Dispatch only: each arm names one action and hands off. The proxy work
//! lives in [`crate::proxy_actions`] so the tray and the autostart path cannot
//! diverge, and the quit path stays in [`crate::lifecycle`] so the tray gets
//! the same hardened shutdown as every other exit.

use std::time::Duration;

use tauri::{AppHandle, Manager};
use tracing::{debug, error};

use crate::app::AppState;
use crate::app::events::{emit_or_log, names};
use crate::lifecycle;
use crate::proxy_actions;
use crate::tray::placement::Anchor;
use crate::tray::{confirm, ids, window};

/// How long to wait for a daemon asked to stop to actually go.
///
/// Matches the budget `lifecycle` gives a hosted daemon, which is in turn
/// sized against the daemon's own 10-second shutdown watchdog.
const SERVICE_STOP_WAIT: Duration = Duration::from_secs(12);

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
        ids::START_SERVICE => spawn_service(app.clone(), true),
        ids::STOP_SERVICE => spawn_service(app.clone(), false),
        ids::COPY_PROXY_URL => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                proxy_actions::copy_endpoint_url(&app).await;
            });
        }
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

        report(&app, result, start, "proxy", "Could not start the proxy");
    });
}

/// Start or stop the daemon off the event thread.
///
/// Stopping waits for it to go before refreshing, so the tray repaints once
/// the teardown has actually finished rather than showing a daemon that is
/// halfway out. Starting is already synchronous — `restart` returns when the
/// new daemon answers.
fn spawn_service(app: AppHandle, start: bool) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();

        let result = if start {
            state.daemon.restart().await
        } else {
            let snapshot = state.snapshot.read().await.clone();
            if !confirm::stop_service(&app, &snapshot) {
                return;
            }

            state.daemon.request_shutdown().await;
            state.daemon.wait_for_exit(SERVICE_STOP_WAIT).await;
            Ok(())
        };

        report(
            &app,
            result,
            start,
            "service",
            "Could not start the gglib service",
        );

        state.refresh.now();
    });
}

/// Log a menu action's outcome, and put a failed *start* on screen.
///
/// Starting is where the interesting failures live — a port already in use, no
/// `gglib` binary to launch — and the daemon's own message names both the
/// cause and the fix, so it is shown verbatim. Stopping is left to the log: it
/// is idempotent on both routes, so a failure there means the thing the user
/// wanted gone is already gone.
fn report(app: &AppHandle, result: Result<(), String>, start: bool, kind: &str, title: &str) {
    let Err(e) = result else {
        return;
    };

    error!(error = %e, start, kind, "Tray action failed");

    if start {
        confirm::report_failure(app, title, &e);
    }
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

/// Confirm before quitting takes a running service with it, then exit.
///
/// The warning describes what will actually happen rather than assuming it.
/// It used to claim quitting stopped the proxy, which stopped being true when
/// the daemon took ownership of the runtime — against an adopted daemon there
/// is nothing to warn about, because it keeps serving.
fn confirm_quit(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let snapshot = state.snapshot.read().await.clone();
        let ends_with_the_app = state.daemon.ownership.ends_with_the_app();

        if !confirm::quit(&app, &snapshot, ends_with_the_app) {
            return;
        }

        // The same entry point Cmd+Q and window close use, so there is exactly
        // one shutdown sequence however the user asked to quit.
        lifecycle::request_shutdown(&app);
    });
}
