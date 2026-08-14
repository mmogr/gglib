//! Application lifecycle and shutdown orchestration.
//!
//! Quit means quit. Close-to-tray is already the verb for "keep serving
//! without a window", so an exit that silently left a daemon holding VRAM —
//! with the tray icon gone and nothing on screen to say so — was one button
//! doing another's job.
//!
//! What quitting ends is decided by [`crate::daemon::Ownership`]: one this app launched
//! or hosts gets its ordered teardown (proxy drain, child shutdown, pidfile
//! audit) triggered over the API and awaited before exit. One that was already
//! answering when the app connected belongs to whoever started it and is left
//! alone.

use crate::app::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tracing::info;

/// How long to wait for the daemon's ordered teardown before exiting anyway.
///
/// Two seconds past the daemon's own 10-second force-exit watchdog, so the
/// app outlasts the deadline the daemon holds itself to rather than racing it.
const TEARDOWN_WAIT: Duration = Duration::from_secs(12);

/// Whether a shutdown has been started, so it is sequenced exactly once.
///
/// `AppHandle::exit` does not end the process directly — it "exits the app by
/// triggering `RunEvent::ExitRequested` and `RunEvent::Exit`". That re-entry is
/// what this flag exists to survive: the `ExitRequested` handler must know the
/// exit it is being asked about is the one we asked for, and let it through
/// rather than preventing it.
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

/// Whether [`request_shutdown`] has already been entered.
///
/// The `RunEvent::ExitRequested` handler calls this to decide whether to
/// prevent the exit: preventing it once is how the cleanup gets to run at all,
/// but preventing the second one — the exit this module itself requested —
/// would strand the process.
pub(crate) fn is_shutting_down() -> bool {
    SHUTTING_DOWN.load(Ordering::SeqCst)
}

/// Whether an incoming `RunEvent::ExitRequested` should be prevented.
///
/// Prevent the first one, so cleanup gets a chance to run before the process
/// goes away. Allow any later one: that is [`request_shutdown`]'s own
/// `exit(0)` coming back around, and preventing it is precisely what left the
/// app running with a dead embedded API server.
pub(crate) const fn should_prevent_exit(shutting_down: bool) -> bool {
    !shutting_down
}

/// Shut the application down, once.
///
/// The single entry point for quitting, shared by every path that can ask for
/// it: `RunEvent::ExitRequested` (Cmd+Q), `WindowEvent::CloseRequested` when
/// close-to-tray is off, and the tray's Quit item. Calls after the first return
/// immediately, so overlapping requests — a Cmd+Q landing while a window close
/// is still cleaning up — collapse into one shutdown instead of racing.
///
/// Spawns rather than blocks: it is called from event-loop handlers that must
/// not stall while llama-server processes are stopped.
pub(crate) fn request_shutdown(app: &AppHandle) {
    if SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
        info!("Shutdown already in progress - ignoring duplicate request");
        return;
    }

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state: tauri::State<'_, AppState> = app.state();
        perform_shutdown(&state).await;
        app.exit(0);
    });
}

/// Perform graceful application shutdown.
///
/// Asks the daemon to shut down over the API — its own ordered teardown runs
/// there, under its own watchdog — and waits, bounded, for it to finish before
/// letting the process exit. Skipped entirely for an adopted daemon, which was
/// serving before this app opened and has no reason to stop because it closed.
///
/// Reach this through [`request_shutdown`] rather than calling it directly.
async fn perform_shutdown(state: &AppState) {
    let ownership = state.daemon.ownership;

    if !ownership.ends_with_the_app() {
        info!(
            ?ownership,
            "Daemon was not ours to stop - leaving it running"
        );
        return;
    }

    info!(?ownership, "Requesting the daemon's teardown before exit");
    state.daemon.request_shutdown().await;
    state.daemon.wait_for_exit(TEARDOWN_WAIT).await;
    info!("Daemon teardown complete");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Ownership;

    /// The first exit request has to be prevented, or cleanup never runs: the
    /// process would go away with llama-server children still alive.
    #[test]
    fn the_first_exit_request_is_prevented() {
        assert!(should_prevent_exit(false));
    }

    /// The second must not be. It is the `exit(0)` at the end of
    /// `request_shutdown` coming back through the event loop, and preventing it
    /// is the bug this guard exists for — the app stayed up with its embedded
    /// API server already aborted, so every window's HTTP call failed.
    #[test]
    fn the_exit_we_asked_for_is_allowed_through() {
        assert!(!should_prevent_exit(true));
    }

    /// A daemon this app started or hosts is this app's to end. The old rule
    /// tore down only the in-process fallback, so the ordinary case — an
    /// external daemon this app launched itself — survived a quit that had
    /// just warned it was stopping the proxy.
    #[test]
    fn quitting_ends_the_daemon_this_app_started() {
        assert!(Ownership::Launched.ends_with_the_app());
        assert!(Ownership::Hosted.ends_with_the_app());
    }

    /// The exception, and the reason ownership is tracked at all: a daemon
    /// that was already serving when the app connected belongs to whoever
    /// started it. `Unresolved` gets the same treatment for the same reason —
    /// this app never established that anything out there is its to stop.
    #[test]
    fn quitting_leaves_a_daemon_we_did_not_start_alone() {
        assert!(!Ownership::Adopted.ends_with_the_app());
        assert!(!Ownership::Unresolved.ends_with_the_app());
    }

    /// The app's wait has to outlast the daemon's own force-exit watchdog, or
    /// it gives up while a teardown that is about to finish is still running.
    #[test]
    fn the_teardown_budget_outlasts_the_daemons_watchdog() {
        assert!(TEARDOWN_WAIT > Duration::from_secs(10));
    }

    /// The guard is a swap, not a load-then-store, so two quit paths firing
    /// together still yield exactly one shutdown.
    #[test]
    fn only_the_first_caller_claims_the_shutdown() {
        let flag = AtomicBool::new(false);

        assert!(!flag.swap(true, Ordering::SeqCst), "first caller proceeds");
        assert!(flag.swap(true, Ordering::SeqCst), "second caller backs off");
        assert!(flag.swap(true, Ordering::SeqCst), "and stays backed off");
    }
}
