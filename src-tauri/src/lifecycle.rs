//! Application lifecycle and shutdown orchestration.

use crate::app::AppState;
use gglib_runtime::pidfile::cleanup_orphaned_servers;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tracing::{error, info, warn};

/// Whether a shutdown has been started, so it is sequenced exactly once.
///
/// `AppHandle::exit` does not end the process directly — it "exits the app by
/// triggering `RunEvent::ExitRequested` and `RunEvent::Exit`". That re-entry is
/// what this flag exists to survive: the `ExitRequested` handler must know the
/// exit it is being asked about is the one we asked for, and let it through
/// rather than preventing it. Without the flag, `prevent_exit` cancels our own
/// exit and the app lingers with its embedded API server already torn down.
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

/// Perform graceful application shutdown with timeout and watchdog.
///
/// # Shutdown sequence
/// 1. Spawn watchdog thread (force exit after 10s)
/// 2. Stop all running llama-server processes (8s timeout)
/// 3. Cancel all active downloads
/// 4. Run PID file audit to catch any stragglers
/// 5. Cancel watchdog and return
///
/// If cleanup exceeds 10 seconds, the watchdog will force `process::exit(1)`.
///
/// Reach this through [`request_shutdown`] rather than calling it directly:
/// this function does the cleanup but nothing to guarantee it happens only
/// once, and running it twice would abort background tasks a second time.
async fn perform_shutdown(state: &AppState) {
    info!("Starting hardened graceful shutdown");

    // Spawn watchdog thread that will force exit after 10 seconds
    let (watchdog_cancel_tx, mut watchdog_cancel_rx) = tokio::sync::oneshot::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        // If we reach here, the channel wasn't cancelled = timeout
        if watchdog_cancel_rx.try_recv().is_err() {
            eprintln!("SHUTDOWN WATCHDOG: Cleanup exceeded 10 seconds - forcing exit");
            std::process::exit(1);
        }
    });

    // Wrap cleanup in 8-second timeout (leaves 2s buffer before watchdog)
    let cleanup_result =
        tokio::time::timeout(Duration::from_secs(8), parallel_cleanup(state)).await;

    match cleanup_result {
        Ok(Ok(())) => info!("Cleanup completed successfully"),
        Ok(Err(e)) => warn!("Cleanup completed with errors: {}", e),
        Err(_) => error!("Cleanup timed out after 8 seconds - proceeding to audit"),
    }

    // Always run PID file audit as final safety net
    info!("Running final PID file audit");
    if let Err(e) = cleanup_orphaned_servers().await {
        error!("PID file audit failed: {}", e);
    }

    // Cancel watchdog - we completed in time
    let _ = watchdog_cancel_tx.send(());

    info!("Hardened graceful shutdown complete");
}

/// Perform parallel cleanup of servers and downloads.
async fn parallel_cleanup(state: &AppState) -> Result<(), String> {
    info!("Stopping all llama-server processes");

    // Abort background tasks first to prevent new events
    {
        let mut tasks = state.background_tasks.write().await;

        if let Some(server_task) = tasks.embedded_server.take() {
            info!("Aborting embedded API server task");
            server_task.abort();
        }

        if let Some(log_task) = tasks.log_emitter.take() {
            info!("Aborting server log emitter task");
            log_task.abort();
        }
    }

    // Run server stop and download cancel in parallel
    let (servers_result, _) = tokio::join!(state.servers.stop_all(), state.downloads.cancel_all());

    // Map server errors to string
    servers_result.map_err(|e| format!("Failed to stop servers: {}", e))
}

/// Perform startup cleanup of orphaned processes.
///
/// Should be called early in the setup phase, before any servers are started.
pub async fn startup_cleanup() {
    info!("Performing startup orphan cleanup");
    if let Err(e) = cleanup_orphaned_servers().await {
        tracing::warn!("Error during startup orphan cleanup: {}", e);
    }
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
