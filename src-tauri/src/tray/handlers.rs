//! Tray menu event handling.
//!
//! Dispatch only: each arm names one action and hands off. The endpoint work
//! lives in [`crate::proxy_actions`] so the tray and the macOS menu cannot
//! diverge, and the quit path stays in [`crate::lifecycle`] so the tray gets
//! the same hardened shutdown as every other exit.

use tauri::{AppHandle, Manager};
use tracing::{debug, error};

use crate::app::AppState;
use crate::app::events::{emit_or_log, names, places};
use crate::lifecycle;
use crate::proxy_actions;
use crate::tray::placement::Anchor;
use crate::tray::{confirm, ids, items, window};

/// What a tray entry does, by its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    ShowPanel,
    CopyEndpointUrl,
    SwitchEndpoint,
    OpenMain,
    Settings,
    SwitchService,
    Quit,
}

/// The action an id names; `None` for the status line, which is a label.
///
/// Pure, so a test can hold every declared id to an action.
fn action_of(id: &str) -> Option<Action> {
    Some(match id {
        ids::SHOW_PANEL => Action::ShowPanel,
        ids::COPY_ENDPOINT_URL => Action::CopyEndpointUrl,
        ids::ENDPOINT_SWITCH => Action::SwitchEndpoint,
        ids::OPEN_MAIN => Action::OpenMain,
        ids::SETTINGS => Action::Settings,
        ids::SERVICE_SWITCH => Action::SwitchService,
        ids::QUIT => Action::Quit,
        _ => return None,
    })
}

/// Perform the action a menu item id names.
///
/// The single entry point for every tray backend, so the tray reaches the same
/// code as the `WebUI` and the CLI however the click arrived.
pub(super) fn dispatch(app: &AppHandle, id: &str) {
    debug!(tray_id = %id, "Tray menu event received");

    let Some(action) = action_of(id) else {
        debug!(tray_id = %id, "Unhandled tray menu event");
        return;
    };

    match action {
        Action::ShowPanel => {
            spawn_ui(app.clone(), |app| {
                window::toggle_panel(&app, Anchor::Unknown)
            });
        }
        Action::OpenMain => spawn_ui(app.clone(), |app| window::show_main(&app)),
        Action::SwitchEndpoint => proxy_actions::spawn_switch(app),
        Action::SwitchService => spawn_service(app.clone()),
        Action::CopyEndpointUrl => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                proxy_actions::copy_endpoint_url(&app).await;
            });
        }
        Action::Settings => open_settings(app),
        Action::Quit => confirm_quit(app),
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

/// Start gglib if the last poll found none, else stop it; off the event
/// thread.
///
/// Stopping asks first when something would be lost, then waits for the
/// daemon to go before refreshing, so the tray repaints once the teardown has
/// actually finished rather than showing a daemon that is halfway out.
/// Starting is already synchronous — `restart` returns when the new daemon
/// answers.
fn spawn_service(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let snapshot = state.snapshot.read().await.clone();
        let start = !items::is_on(ids::SERVICE_SWITCH, &snapshot);

        let result = if start {
            state.daemon.restart().await
        } else {
            if !confirm::stop_service(&app, &snapshot) {
                return;
            }

            state.daemon.request_shutdown().await;
            state.daemon.wait_for_exit(lifecycle::TEARDOWN_WAIT).await;
            Ok(())
        };

        report(&app, result, start, "Could not start gglib");

        state.refresh.now();
    });
}

/// Log the service switch's outcome, and put a failed *start* on screen.
///
/// Starting is where the interesting failures live — a port already in use, no
/// `gglib` binary to launch — and the daemon's own message names both the
/// cause and the fix, so it is shown verbatim. Stopping is left to the log: a
/// failure there means the thing the user wanted gone is already gone.
fn report(app: &AppHandle, result: Result<(), String>, start: bool, title: &str) {
    let Err(e) = result else {
        return;
    };

    error!(error = %e, start, "Tray service action failed");

    if start {
        confirm::report_failure(app, title, &e);
    }
}

/// Bring the main window forward and send it to Settings › General.
///
/// Showing the window first matters: with close-to-tray on, Settings… may be
/// chosen while nothing is on screen to open it in.
fn open_settings(app: &AppHandle) {
    if let Err(e) = window::show_main(app) {
        error!(error = %e, "Failed to show main window for settings");
    }
    emit_or_log(app, names::MENU_NAVIGATE, places::SETTINGS);
}

/// Confirm before quitting takes a running service with it, then exit.
///
/// The warning describes what will actually happen rather than assuming it:
/// against an adopted daemon there is nothing to warn about, because it keeps
/// serving.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every id but the status line does something, and no two do the same
    /// thing: an entry that renders and then does nothing is the bug TM-2 was.
    #[test]
    fn every_id_is_routed_to_its_own_action() {
        let mut actions: Vec<Action> = Vec::new();
        for id in ids::ALL {
            match action_of(id) {
                None => assert_eq!(id, ids::STATUS, "{id} is not routed"),
                Some(action) => {
                    assert!(!actions.contains(&action), "{id} shares an action");
                    actions.push(action);
                }
            }
        }
        assert_eq!(actions.len(), ids::ALL.len() - 1);
    }
}
