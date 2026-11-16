//! Utility commands.
//!
//! These are non-domain-specific commands that don't fit elsewhere.

use crate::app::AppState;
use crate::daemon::Service;
use crate::menu::state_sync;
use gglib_core::utils::process::foreground::Foreground;
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

/// Get backend API info (its port and token), once the app knows its daemon.
///
/// The frontend calls this at startup, and again after the daemon refused its
/// token (a restart mints a new one), to learn where the API lives and what
/// to present. While the app's launch of the daemon (or a Start gglib) is in
/// flight the answer waits for it, so the page's first requests go to a
/// daemon that listens, with the token it minted; the launch's own budget
/// bounds the wait. A launch that failed is the error, in its own words.
#[tauri::command]
pub(crate) async fn get_embedded_api_info(
    state: tauri::State<'_, AppState>,
) -> Result<ApiInfo, String> {
    api_info(&state.daemon.service, crate::daemon::daemon_token).await
}

/// The answer to the page's "where is the service?": once no launch is in
/// flight, the daemon's port and its token, read from `token` then, since a
/// daemon started from a terminal since mints a new one.
async fn api_info(
    service: &Service,
    token: impl FnOnce() -> Option<String>,
) -> Result<ApiInfo, String> {
    service.settled().await?;
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
    // Said inline by the window, whoever ran the launch; a dialog for it is
    // the launch owner's (`Started::failure_to_report`).
    let started = state.daemon.restart().await.result();
    state.refresh.now();
    started
}

/// Let the service the app talks to bring a window to the front: the page
/// asks as it posts a repair (#2046).
///
/// The service runs the installer of Microsoft's Visual C++ runtime, which
/// asks Windows for permission, and Windows leaves a prompt asked for from
/// the background flashing on the taskbar. This app's window has just been
/// clicked, so it can lend the service the right to come to the front. The
/// prompt comes only after the service has downloaded the installer (about
/// 25 MB) and checked it, and the right ends at the user's next input to
/// any other program, this app's window included (the service has none):
/// a click or key press during the download ends it. Answers whether the
/// grant was made; a grant not made changes nothing else, and the repair is
/// posted as before.
///
/// Never waits for the service: a launch or a Start gglib in flight, or a
/// service the last poll did not reach, is no grant, at once. The poll is
/// asked rather than the lock, which `DaemonLock::is_held` would take for
/// an instant (`daemon/holder.rs` says why, and what that leaves).
#[tauri::command]
pub(crate) async fn allow_service_foreground(
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let reachable = state.snapshot.read().await.reachable;
    Ok(allow_service(
        reachable,
        crate::daemon::service_pid,
        gglib_core::utils::process::foreground::allow_foreground,
    ))
}

/// When the last poll reached the service (`reachable`), ask `allow` to
/// grant the process `pid` names; whether it did. Otherwise nothing is
/// read or asked.
fn allow_service(
    reachable: bool,
    pid: impl FnOnce() -> Option<u32>,
    allow: impl FnOnce(u32) -> std::io::Result<Foreground>,
) -> bool {
    if !reachable {
        return false;
    }
    let Some(pid) = pid() else {
        return false;
    };
    match allow(pid) {
        Ok(Foreground::Allowed) => {
            tracing::info!(
                pid,
                "the service may come to the front for Windows' permission prompt"
            );
            true
        }
        Ok(Foreground::NotNeeded) => false,
        Err(e) => {
            tracing::warn!(
                pid,
                error = %e,
                "the service could not be let to come to the front; Windows may leave its permission prompt on the taskbar"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::daemon::{DaemonSnapshot, LAUNCH_WAIT, Ownership};

    /// The daemon's token file, as the answer reads it. A launch that comes
    /// up rewrites it, so a test can tell an answer read before the launch
    /// settled from one read after.
    #[derive(Clone)]
    struct TokenFile(Arc<Mutex<&'static str>>);

    impl TokenFile {
        fn holding(token: &'static str) -> Self {
            Self(Arc::new(Mutex::new(token)))
        }

        fn mint(&self, token: &'static str) {
            *self.0.lock().expect("the token file") = token;
        }

        fn read(&self) -> String {
            (*self.0.lock().expect("the token file")).to_owned()
        }
    }

    /// Whether `task` is still waiting after far longer than any launch may
    /// take: the clock is paused, so this runs at once, and an answer given
    /// after any fixed delay would be seen.
    async fn waits<T>(task: &tokio::task::JoinHandle<T>) -> bool {
        tokio::time::sleep(LAUNCH_WAIT * 4).await;
        !task.is_finished()
    }

    /// Whether `task` has answered with no time passing at all.
    async fn answered_at_once<T>(task: &tokio::task::JoinHandle<T>) -> bool {
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
        task.is_finished()
    }

    /// The page asking where the service is, as `get_embedded_api_info`
    /// does, with the token file as it reads when the answer is given.
    fn ask(
        service: &Arc<Service>,
        file: &TokenFile,
    ) -> tokio::task::JoinHandle<Result<ApiInfo, String>> {
        let (service, file) = (Arc::clone(service), file.clone());
        tokio::spawn(async move { api_info(&service, || Some(file.read())).await })
    }

    fn answer(token: &str) -> ApiInfo {
        ApiInfo {
            port: gglib_core::daemon_port(),
            token: Some(token.to_owned()),
        }
    }

    /// While the launch is in flight the page's question waits, however
    /// long the launch takes: not an error (the page would say gglib is not
    /// running), not the port before anything listens on it, not a token the
    /// service has not minted yet.
    #[tokio::test(start_paused = true)]
    async fn the_answer_waits_while_the_launch_is_in_flight() {
        let (service, _launch) = Service::launching();
        let service = Arc::new(service);
        let file = TokenFile::holding("old");
        let asked = [ask(&service, &file), ask(&service, &file)];
        for task in &asked {
            assert!(waits(task).await, "answered before the launch settled");
        }
    }

    /// The launch settling on a running service answers everyone who asked
    /// with its address and the token it minted, read after it came up.
    #[tokio::test(start_paused = true)]
    async fn a_launch_that_comes_up_answers_every_waiter() {
        let (service, launch) = Service::launching();
        let service = Arc::new(service);
        let file = TokenFile::holding("the last daemon's");
        let asked = [ask(&service, &file), ask(&service, &file)];
        assert!(waits(&asked[0]).await, "answered before the launch settled");

        file.mint("minted");
        launch.settle(Ok(Ownership::Launched));
        for task in asked {
            assert_eq!(task.await.expect("the request"), Ok(answer("minted")));
        }
    }

    /// The launch failing answers everyone who asked with why, in the words
    /// the launch gave.
    #[tokio::test(start_paused = true)]
    async fn a_launch_that_fails_answers_every_waiter_with_why() {
        let (service, launch) = Service::launching();
        let service = Arc::new(service);
        let file = TokenFile::holding("none");
        let asked = [ask(&service, &file), ask(&service, &file)];
        let why = "the gglib daemon did not come up within 15s";
        launch.settle(Err(why.to_owned()));
        for task in asked {
            assert_eq!(task.await.expect("the request"), Err(why.to_owned()));
        }
    }

    /// Once settled, a question is answered at once.
    #[tokio::test(start_paused = true)]
    async fn a_question_after_the_launch_is_answered_at_once() {
        let (service, launch) = Service::launching();
        let service = Arc::new(service);
        launch.settle(Ok(Ownership::Adopted));
        let asked = ask(&service, &TokenFile::holding("minted"));
        assert!(
            answered_at_once(&asked).await,
            "a settled launch keeps nobody waiting"
        );
        assert_eq!(asked.await.expect("the request"), Ok(answer("minted")));
    }

    /// A service started again (Start gglib) mints a new token: a question
    /// asked during the restart waits for it, and is answered with the new
    /// token, never the one the stopped service had.
    #[tokio::test(start_paused = true)]
    async fn a_question_during_a_restart_waits_for_the_new_service() {
        let (service, launch) = Service::launching();
        let service = Arc::new(service);
        let file = TokenFile::holding("first");
        launch.settle(Ok(Ownership::Launched));
        assert_eq!(
            ask(&service, &file).await.expect("the request"),
            Ok(answer("first"))
        );

        let restart = service.begin().expect("nothing is in flight");
        let asked = ask(&service, &file);
        assert!(waits(&asked).await, "answered before the restart settled");

        file.mint("second");
        restart.settle(Ok(Ownership::Launched));
        assert_eq!(asked.await.expect("the request"), Ok(answer("second")));
    }

    /// The process ids the process module was asked to allow, and what it
    /// answers each time.
    struct Grants {
        asked: Mutex<Vec<u32>>,
        answer: fn() -> std::io::Result<Foreground>,
    }

    impl Grants {
        fn answering(answer: fn() -> std::io::Result<Foreground>) -> Self {
            Self {
                asked: Mutex::new(Vec::new()),
                answer,
            }
        }

        fn allow(&self, pid: u32) -> std::io::Result<Foreground> {
            self.asked.lock().expect("the grants").push(pid);
            (self.answer)()
        }

        fn asked(&self) -> Vec<u32> {
            self.asked.lock().expect("the grants").clone()
        }
    }

    /// What the last poll says of a service it reached.
    fn reached() -> bool {
        DaemonSnapshot {
            reachable: true,
            ..DaemonSnapshot::default()
        }
        .reachable
    }

    /// The service the app talks to is the one allowed, by the id its
    /// record gives, and the page hears that the grant was made.
    #[test]
    fn the_service_the_app_talks_to_is_allowed() {
        let grants = Grants::answering(|| Ok(Foreground::Allowed));
        let granted = allow_service(reached(), || Some(4242), |pid| grants.allow(pid));
        assert!(granted);
        assert_eq!(grants.asked(), [4242]);
    }

    /// With no service known (no record of a running one), nothing is
    /// asked and nothing is granted.
    #[test]
    fn no_known_service_asks_nothing() {
        let grants = Grants::answering(|| Ok(Foreground::Allowed));
        let granted = allow_service(reached(), || None, |pid| grants.allow(pid));
        assert!(!granted);
        assert_eq!(grants.asked(), Vec::<u32>::new());
    }

    /// A launch or a Start gglib in flight, and a service the last poll did
    /// not reach (a failed launch, a stopped service), are no grant at
    /// once: nothing waits for the launch, the record is not even read.
    #[test]
    fn a_service_not_reached_asks_nothing_and_waits_for_nothing() {
        for snapshot in [DaemonSnapshot::starting(), DaemonSnapshot::default()] {
            let grants = Grants::answering(|| Ok(Foreground::Allowed));
            let granted = allow_service(
                snapshot.reachable,
                || panic!("the record was read with no service reached"),
                |pid| grants.allow(pid),
            );
            assert!(!granted);
            assert_eq!(grants.asked(), Vec::<u32>::new());
        }
    }

    /// Windows refusing, and an OS with no such right, are both no grant:
    /// the page then posts the repair as it would have.
    #[test]
    fn a_refusal_or_no_such_right_is_no_grant() {
        let refused = Grants::answering(|| Err(std::io::Error::from_raw_os_error(5)));
        assert!(!allow_service(
            reached(),
            || Some(4242),
            |pid| refused.allow(pid)
        ));
        assert_eq!(refused.asked(), [4242]);

        let none = Grants::answering(|| Ok(Foreground::NotNeeded));
        assert!(!allow_service(
            reached(),
            || Some(4242),
            |pid| none.allow(pid)
        ));
        assert_eq!(none.asked(), [4242]);
    }
}
