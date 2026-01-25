//! Application lifecycle and shutdown orchestration.

use crate::app::AppState;
use gglib_runtime::pidfile::cleanup_orphaned_servers;
use std::time::Duration;
use tracing::{error, info, warn};

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
/// This should be called from both:
/// - `RunEvent::ExitRequested` (Cmd+Q, app quit)
/// - `WindowEvent::CloseRequested` (window close button)
pub async fn perform_shutdown(state: &AppState) {
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
    let (servers_result, _) = tokio::join!(
        state.gui.stop_all_servers(),
        state.gui.cancel_all_downloads()
    );

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
