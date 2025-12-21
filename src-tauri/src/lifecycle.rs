//! Application lifecycle and shutdown orchestration.

use crate::app::AppState;
use gglib_runtime::pidfile::cleanup_orphaned_servers;
use tracing::info;

/// Perform graceful application shutdown.
///
/// # Shutdown sequence
/// 1. Stop all running llama-server processes
/// 2. Cancel all active downloads
///
/// This should be called from both:
/// - `RunEvent::ExitRequested` (Cmd+Q, app quit)
/// - `WindowEvent::CloseRequested` (window close button)
pub async fn perform_shutdown(state: &AppState) {
    info!("Starting graceful shutdown");

    // Stop all servers first (most critical)
    info!("Stopping all llama-server processes");
    if let Err(e) = state.gui.stop_all_servers().await {
        tracing::warn!("Error stopping servers during shutdown: {}", e);
    }

    // Cancel downloads
    info!("Canceling all downloads");
    state.gui.cancel_all_downloads().await;

    info!("Graceful shutdown complete");
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
