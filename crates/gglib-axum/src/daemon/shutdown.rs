//! Ordered daemon teardown, and the signal future that triggers it.
//!
//! The ordering matters: the proxy is drained first so no request is
//! mid-flight when its upstream dies, then every llama-server child is
//! stopped through the graceful SIGTERM → grace → SIGKILL path, then a
//! final pidfile audit catches anything that slipped through. The whole
//! sequence runs under a force-exit watchdog so a wedged child (D-state on
//! a blocked CUDA ioctl) cannot keep the daemon alive forever.

use std::future::Future;
use std::time::Duration;

use crate::state::AppState;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// How long the whole teardown may take before the watchdog force-exits.
const SHUTDOWN_WATCHDOG: Duration = Duration::from_secs(10);

/// Resolve when *either* trigger fires, then cancel the token so both converge.
///
/// The cancel is the whole point, and its absence was a real bug. The daemon has
/// two ways to stop — a signal, or `POST /api/daemon/shutdown` — and the select
/// alone only *observed* the token. On the signal path the token therefore stayed
/// live, so anything bounded by it never ended: `/api/events` streams held
/// `with_graceful_shutdown` open, `perform_shutdown` never ran, and the liveness
/// watchdog force-exited later. Ctrl-C, `systemctl stop` and `kill` all take
/// that path; `POST /api/daemon/shutdown` cancels the token itself and so was
/// unaffected.
///
/// `CancellationToken::cancel` is idempotent, so firing it on the API path too
/// costs nothing. gglib-proxy avoids this shape entirely by passing one token to
/// both sides (`gglib-proxy/src/server.rs`); the daemon needs a select because
/// only one of its two triggers is a token.
pub(crate) async fn await_shutdown<S>(signal: S, token: CancellationToken)
where
    S: Future<Output = ()>,
{
    tokio::select! {
        () = signal => info!("shutdown signal received"),
        () = token.cancelled() => info!("shutdown requested over the API"),
    }
    token.cancel();
}

/// Resolve when the process is asked to stop.
///
/// Completes on SIGINT (Ctrl-C) on every platform, and additionally on
/// SIGTERM on Unix, which is what a service manager sends.
pub(crate) async fn shutdown_signal() {
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
pub(super) async fn perform_shutdown(state: &AppState) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    info!("daemon shutting down");

    // If teardown wedges — typically a llama-server stuck in an
    // uninterruptible CUDA ioctl — exit anyway. Children the watchdog
    // abandons are caught by the next daemon start's orphan sweep.
    //
    // An OS thread, not a tokio task, and non-negotiably so: the wedge being
    // guarded against can be the runtime itself (every worker blocked, as in
    // the #721 admission deadlock), and a watchdog that needs a free worker
    // to fire is starved by exactly the condition it exists to escape. The
    // thread cannot be aborted, so completion is signalled through a flag it
    // checks on waking; a disarmed watchdog simply returns.
    let completed = Arc::new(AtomicBool::new(false));
    let disarm = Arc::clone(&completed);
    std::thread::spawn(move || {
        std::thread::sleep(SHUTDOWN_WATCHDOG);
        if disarm.load(Ordering::Acquire) {
            return;
        }
        warn!("shutdown watchdog fired — forcing exit");
        std::process::exit(1);
    });

    // 0. Take the remote tunnel down first, so nothing new arrives from
    //    outside while the rest is dismantled; its ticket dies here. "Not
    //    enabled" is the usual answer.
    if let Err(e) = state.remote.disable().await {
        tracing::debug!("remote disable during shutdown: {e}");
    }
    //    And the connect side, so a client on the loopback port hears a
    //    closed socket now rather than a dead one later.
    if let Err(e) = state.remote.disconnect().await {
        tracing::debug!("remote disconnect during shutdown: {e}");
    }

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

    completed.store(true, Ordering::Release);
    info!("daemon shutdown complete");
}

#[cfg(test)]
#[path = "shutdown_tests.rs"]
mod shutdown_tests;
