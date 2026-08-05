//! Application lifecycle and shutdown orchestration.
//!
//! The daemon owns llama-server, downloads and pidfiles, so quitting the
//! desktop app tears down almost nothing. The one exception is the
//! in-process daemon fallback: when this app is hosting the daemon itself,
//! quitting the app is quitting the daemon, so its ordered teardown (proxy
//! drain, child shutdown, pidfile audit) is triggered over the API and
//! awaited before exit.

use crate::app::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tracing::info;

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
pub fn is_shutting_down() -> bool {
    SHUTTING_DOWN.load(Ordering::SeqCst)
}

/// Whether an incoming `RunEvent::ExitRequested` should be prevented.
///
/// Prevent the first one, so cleanup gets a chance to run before the process
/// goes away. Allow any later one: that is [`request_shutdown`]'s own
/// `exit(0)` coming back around, and preventing it is precisely what left the
/// app running with a dead embedded API server.
pub const fn should_prevent_exit(shutting_down: bool) -> bool {
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
pub fn request_shutdown(app: &AppHandle) {
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
/// With an external daemon there is nothing backend-shaped to stop — the
/// daemon and its llama-servers deliberately outlive the app. With the
/// in-process fallback, this app *is* the daemon: ask it to shut down over
/// the API (its own ordered teardown runs — proxy drain, graceful child
/// stop, pidfile audit, all under its watchdog) and wait, bounded, for it
/// to finish before letting the process exit.
///
/// Reach this through [`request_shutdown`] rather than calling it directly.
async fn perform_shutdown(state: &AppState) {
    if !state.daemon.hosted_in_process {
        info!("External daemon stays running - nothing to tear down");
        return;
    }

    info!("Hosted daemon: requesting its ordered teardown before exit");
    state.daemon.request_shutdown().await;
    state.daemon.wait_for_exit(Duration::from_secs(12)).await;
    info!("Hosted daemon teardown complete");
}

#[cfg(test)]
mod tests {
    use super::*;

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
