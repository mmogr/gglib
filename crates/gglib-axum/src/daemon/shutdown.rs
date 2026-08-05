//! Ordered daemon teardown, and the signal future that triggers it.
//!
//! The ordering matters: the proxy is drained first so no request is
//! mid-flight when its upstream dies, then every llama-server child is
//! stopped through the graceful SIGTERM → grace → SIGKILL path, then a
//! final pidfile audit catches anything that slipped through. The whole
//! sequence runs under a force-exit watchdog so a wedged child (D-state on
//! a blocked CUDA ioctl) cannot keep the daemon alive forever.

use std::time::Duration;

use crate::state::AppState;
use tracing::{info, warn};

/// How long the whole teardown may take before the watchdog force-exits.
const SHUTDOWN_WATCHDOG: Duration = Duration::from_secs(10);

/// Resolve when the process is asked to stop.
///
/// Completes on SIGINT (Ctrl-C) on every platform, and additionally on
/// SIGTERM on Unix, which is what a service manager sends.
pub async fn shutdown_signal() {
    let interrupt = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            warn!("could not listen for Ctrl-C ({e}); shutdown will not be graceful");
            // Never resolve: let the other branch (or the server) decide.
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    {
        let terminate = async {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut sig) => {
                    sig.recv().await;
                }
                Err(e) => {
                    warn!("could not listen for SIGTERM ({e})");
                    std::future::pending::<()>().await;
                }
            }
        };

        tokio::select! {
            () = interrupt => {}
            () = terminate => {}
        }
    }

    #[cfg(not(unix))]
    interrupt.await;
}

/// Tear the daemon down in order, under a force-exit watchdog.
///
/// Every step is best-effort: a failing step is logged and the next one
/// still runs, because each protects a different resource (proxy clients,
/// llama-server children, download partials, pidfiles).
pub async fn perform_shutdown(state: &AppState) {
    info!("daemon shutting down");

    // If teardown wedges — typically a llama-server stuck in an
    // uninterruptible CUDA ioctl — exit anyway. Children the watchdog
    // abandons are caught by the next daemon start's orphan sweep.
    let watchdog = tokio::spawn(async {
        tokio::time::sleep(SHUTDOWN_WATCHDOG).await;
        warn!("shutdown watchdog fired — forcing exit");
        std::process::exit(1);
    });

    // 1. Drain the proxy so in-flight requests finish before their upstream
    //    dies. "Not running" is a fine answer.
    if let Err(e) = state.proxy.stop().await {
        tracing::debug!("proxy stop during shutdown: {e}");
    }

    // 2. Stop every llama-server child through the graceful path
    //    (SIGTERM → grace → SIGKILL → bounded reap) and delete pidfiles.
    if let Err(e) = state.servers.stop_all().await {
        warn!("stopping llama-server children during shutdown: {e}");
    }

    // 3. Cancel queued/active downloads so partial files are accounted for.
    state.downloads.cancel_all().await;

    // 4. Final audit: anything still recorded in the pidfile directory is an
    //    orphan by definition now.
    if let Err(e) = gglib_runtime::pidfile::cleanup_orphaned_servers().await {
        warn!("final orphan audit failed: {e}");
    }

    watchdog.abort();
    info!("daemon shutdown complete");
}
