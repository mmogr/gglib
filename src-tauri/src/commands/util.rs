//! Utility commands.
//!
//! These are non-domain-specific commands that don't fit elsewhere.

use crate::app::AppState;
use crate::menu::state_sync;
use tauri::AppHandle;

/// Where the frontend reaches the backend API, the daemon's fixed loopback
/// port, and the daemon's token, which every `/api` route asks for (#983).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ApiInfo {
    /// Port of the daemon's management API.
    pub port: u16,
    /// The daemon's token, or `None` when there is none to read yet.
    pub token: Option<String>,
}

/// Get backend API info (its port and token).
///
/// The frontend calls this at startup, and again after the daemon refused its
/// token (a restart mints a new one), to learn where the API lives and what
/// to present.
#[tauri::command]
pub(crate) fn get_embedded_api_info() -> ApiInfo {
    ApiInfo {
        port: gglib_core::daemon_port(),
        token: crate::daemon::daemon_token(),
    }
}

/// The answer to the page's "where is the service?", as today: at once,
/// whatever the launch is doing.
pub(crate) async fn api_info(
    _service: &crate::daemon::Service,
    token: impl FnOnce() -> Option<String>,
) -> Result<ApiInfo, String> {
    Ok(ApiInfo {
        port: gglib_core::daemon_port(),
        token: token(),
    })
}

/// Open a URL in the system's default browser.
///
/// Used by the frontend to open external links (e.g., `HuggingFace` model pages).
#[tauri::command]
pub(crate) async fn open_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| format!("Failed to open URL: {e}"))
}

/// Set the currently selected model ID and sync menu state.
///
/// The selection lives in this process, not in the daemon, so repainting from
/// the snapshot already in hand is correct here — nothing about the daemon
/// changed.
#[tauri::command]
pub(crate) async fn set_selected_model(
    model_id: Option<i64>,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // Update selected model ID
    *state.selected_model_id.write().await = model_id;

    // Sync menu state
    state_sync::sync_all_state(&app, &state).await
}

/// Sync menu state after the frontend has changed something.
///
/// Every caller is a fire-and-forget `syncMenuStateSilent()` after a server
/// was stopped or a model removed. The menu paints from
/// `AppState::snapshot`, which only `daemon::watch` writes, so this asks for
/// a poll rather than painting: a paint now would redraw what was true
/// *before* the action, and it would stay wrong until the next tick. The
/// poll repaints once it sees the change. This is the frontend's equivalent
/// of the Rust-side callers' `Refresh::now`. The one thing the menu reads
/// that the daemon does not report, the selected model, is painted by
/// `set_selected_model` as it is written.
#[tauri::command]
pub(crate) async fn sync_menu_state(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.refresh.now();
    Ok(())
}

/// Show and focus the main window: the tray panel's "Open gglib".
///
/// The same `show_main` the tray menu's Open gglib, the icon's double-click
/// and the dock's reopen use, so the Dock icon comes back and a minimised
/// window is restored whichever way it was asked for. The panel's one window
/// action; everything else it does is HTTP. Async, so it runs off the
/// event thread, as the tray menu's Open gglib does.
#[tauri::command]
pub(crate) async fn show_main_window(app: AppHandle) -> Result<(), String> {
    crate::tray::window::show_main(&app).map_err(|e| format!("Could not show the main window: {e}"))
}

/// Start the gglib daemon: the main window's Start gglib, on the screen it
/// shows while the daemon is not running.
///
/// The same `Daemon::restart` the tray menu's Start gglib runs, so the two
/// start a daemon one way; it returns once the new daemon answers. The
/// refusal is the daemon handle's own words (one already running, the port
/// held by another program, no `gglib` binary to launch), which the window
/// shows as they are. The tray is asked to repaint either way.
#[tauri::command]
pub(crate) async fn start_gglib(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let started = state.daemon.restart().await;
    state.refresh.now();
    started
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::daemon::{Ownership, Service};

    /// Whether `task` is still waiting once everything runnable has run.
    async fn waits<T>(task: &tokio::task::JoinHandle<T>) -> bool {
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
        !task.is_finished()
    }

    /// The page asking where the service is, as `get_embedded_api_info`
    /// does, with the token file reading `token` when it answers.
    fn ask(
        service: &Arc<Service>,
        token: &'static str,
    ) -> tokio::task::JoinHandle<Result<ApiInfo, String>> {
        let service = Arc::clone(service);
        tokio::spawn(async move { api_info(&service, || Some(token.to_owned())).await })
    }

    fn answer(token: &str) -> ApiInfo {
        ApiInfo {
            port: gglib_core::daemon_port(),
            token: Some(token.to_owned()),
        }
    }

    /// While the launch is in flight the page's question waits: not an
    /// error (the page would say gglib is not running), not the port before
    /// anything listens on it, not a token the service has not minted yet.
    #[tokio::test]
    async fn the_answer_waits_while_the_launch_is_in_flight() {
        let (service, _launch) = Service::launching();
        let service = Arc::new(service);
        let asked = [ask(&service, "old"), ask(&service, "old")];
        for task in &asked {
            assert!(waits(task).await, "answered before the launch settled");
        }
    }

    /// The launch settling on a running service answers everyone who asked
    /// with its address and the token it minted.
    #[tokio::test]
    async fn a_launch_that_comes_up_answers_every_waiter() {
        let (service, launch) = Service::launching();
        let service = Arc::new(service);
        let asked = [ask(&service, "minted"), ask(&service, "minted")];
        launch.settle(Ok(Ownership::Launched));
        for task in asked {
            assert_eq!(task.await.expect("the request"), Ok(answer("minted")));
        }
    }

    /// The launch failing answers everyone who asked with why, in the words
    /// the launch gave.
    #[tokio::test]
    async fn a_launch_that_fails_answers_every_waiter_with_why() {
        let (service, launch) = Service::launching();
        let service = Arc::new(service);
        let asked = [ask(&service, "none"), ask(&service, "none")];
        let why = "the gglib daemon did not come up within 15s";
        launch.settle(Err(why.to_owned()));
        for task in asked {
            assert_eq!(task.await.expect("the request"), Err(why.to_owned()));
        }
    }

    /// Once settled, a question is answered at once.
    #[tokio::test]
    async fn a_question_after_the_launch_is_answered_at_once() {
        let (service, launch) = Service::launching();
        let service = Arc::new(service);
        launch.settle(Ok(Ownership::Adopted));
        let asked = ask(&service, "minted");
        assert!(
            !waits(&asked).await,
            "a settled launch keeps nobody waiting"
        );
        assert_eq!(asked.await.expect("the request"), Ok(answer("minted")));
    }

    /// A service started again (Start gglib) mints a new token: a question
    /// asked during the restart waits for it, and is answered with the new
    /// token, never the one the stopped service had.
    #[tokio::test]
    async fn a_question_during_a_restart_waits_for_the_new_service() {
        let (service, launch) = Service::launching();
        let service = Arc::new(service);
        launch.settle(Ok(Ownership::Launched));
        assert_eq!(
            ask(&service, "first").await.expect("the request"),
            Ok(answer("first"))
        );

        let restart = service.begin().expect("nothing is in flight");
        let asked = ask(&service, "second");
        assert!(waits(&asked).await, "answered before the restart settled");

        restart.settle(Ok(Ownership::Launched));
        assert_eq!(asked.await.expect("the request"), Ok(answer("second")));
    }
}
