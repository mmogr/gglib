//! Keeping the app's picture of the daemon true.
//!
//! Nothing in this process used to ask the daemon anything. Proxy state was
//! whatever the tray menu had last done itself, so a proxy brought up by
//! `proxy_autostart` — the daemon's job since the daemon consolidation — or by
//! the CLI, or by the window, left the tray permanently wrong.
//!
//! One task polls, and it is the **only** writer of [`DaemonSnapshot`]. That
//! matters: an optimistic write next to a poll is a lost update waiting to
//! happen, because a request that read before an action can land after it.
//! Callers that change something ask for an immediate poll via [`Refresh`]
//! instead of publishing a guess.
//!
//! Polling rather than subscribing to the daemon's `/api/events` stream is
//! deliberate. The server lifecycle events are deltas; `gglib-sse`'s
//! broadcaster documents that it drops them silently for a lagging subscriber,
//! and the one `ServerSnapshot` is emitted at daemon startup rather than per
//! subscriber — so a single missed event would leave the resident count wrong
//! until the app restarted. Every poll is absolute, so drift cannot accumulate;
//! a failed request is also the daemon-down signal, and it catches a wedged
//! daemon that a 30-second SSE keep-alive would still call healthy.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tokio::sync::Notify;
use tracing::debug;

use crate::app::AppState;
use crate::daemon::{Daemon, DaemonSnapshot};
use crate::lifecycle;
use crate::menu::state_sync::sync_all_state_logged;

/// How often to ask the daemon what it is doing.
///
/// Only externally-driven changes wait this long; anything the app does itself
/// calls [`Refresh::now`]. Two seconds is under the threshold at which a menu
/// bar icon reads as stale, and a repaint only happens when something actually
/// changed.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// A handle for asking the watcher to poll now rather than on the next tick.
#[derive(Clone, Default)]
pub(crate) struct Refresh(Arc<Notify>);

impl Refresh {
    /// Wake the watcher immediately.
    ///
    /// Called after this app changes something, so the tray does not sit out
    /// the poll interval showing what the user just stopped doing.
    pub(crate) fn now(&self) {
        self.0.notify_one();
    }

    /// Wait for the next poll: the interval, or sooner if [`Self::now`] fired.
    ///
    /// `timeout` rather than `select!`, which would pull tokio's `macros`
    /// feature in for one call site.
    async fn wait(&self) {
        let _ = tokio::time::timeout(POLL_INTERVAL, self.0.notified()).await;
    }
}

/// Start watching the daemon, repainting every surface whenever it changes.
///
/// Polls once immediately so the first paint shows the daemon's real state
/// rather than the struct defaults — which is what used to leave a launch with
/// `proxy_autostart` on reading "proxy stopped".
pub(crate) fn spawn(app: &AppHandle) {
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        let refresh = app.state::<AppState>().refresh.clone();

        loop {
            poll_once(&app).await;
            refresh.wait().await;

            // Stop before the repaint rather than after: a snapshot taken
            // while the daemon is being torn down would paint the tray on its
            // way out for nobody's benefit.
            if lifecycle::is_shutting_down() {
                return;
            }
        }
    });
}

/// One poll: read the daemon, and repaint only if anything changed.
async fn poll_once(app: &AppHandle) {
    let state = app.state::<AppState>();
    let next = read(&state.daemon).await;

    {
        let mut current = state.snapshot.write().await;
        if *current == next {
            return;
        }
        debug!(?next, "Daemon state changed");
        *current = next;
    }

    // Deliberately outside the guard above. Syncing awaits the tray, which on
    // Linux is a D-Bus round trip, and holding a write lock across it would
    // stall every reader for the duration.
    sync_all_state_logged(app, &state).await;
}

/// Ask the daemon what it is doing.
///
/// Either request failing means the daemon is not answering, which is a state
/// in its own right rather than an error to log and drop: it is what the tray
/// shows as "not running".
async fn read(daemon: &Daemon) -> DaemonSnapshot {
    let proxy = daemon.get_json("/api/proxy/status").await;
    let servers = daemon.get_json("/api/servers").await;

    match (proxy, servers) {
        (Ok(proxy), Ok(servers)) => DaemonSnapshot::from_responses(&proxy, &servers),
        _ => DaemonSnapshot::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clone has to reach the same waiter. `Refresh` is handed out by
    /// cloning it off `AppState`, so a clone that notified its own private
    /// `Notify` would make every caller's `now()` a silent no-op — and that
    /// failure looks exactly like the stale-surface bug this handle exists to
    /// prevent.
    #[tokio::test(start_paused = true)]
    async fn a_clone_wakes_the_original() {
        let refresh = Refresh::default();
        let clone = refresh.clone();

        clone.now();

        // Returns without the interval elapsing, so the wake came from `now()`.
        tokio::time::timeout(Duration::from_millis(1), refresh.wait())
            .await
            .expect("a clone's notify must reach the original's waiter");
    }

    /// `notify_one` stores a permit when nobody is waiting. That is what makes
    /// the ordering safe: an action can call `now()` before the watcher gets
    /// back to `wait()`, which is the ordinary case for a fast action.
    #[tokio::test(start_paused = true)]
    async fn a_wake_that_arrives_early_is_not_lost() {
        let refresh = Refresh::default();

        refresh.now();

        tokio::time::timeout(Duration::from_millis(1), refresh.wait())
            .await
            .expect("a notify with no waiter must be held, not dropped");
    }

    /// With nothing asking, the watcher still polls — that is what catches a
    /// proxy the CLI started, or a daemon that died.
    #[tokio::test(start_paused = true)]
    async fn the_interval_wakes_it_on_its_own() {
        let refresh = Refresh::default();

        let start = tokio::time::Instant::now();
        refresh.wait().await;

        assert!(
            start.elapsed() >= POLL_INTERVAL,
            "an unprompted wait must run the full interval"
        );
    }

    /// One permit, one wake. A stored notification must not satisfy the next
    /// wait as well, or the watcher spins on a single `now()`.
    #[tokio::test(start_paused = true)]
    async fn one_wake_does_not_satisfy_two_waits() {
        let refresh = Refresh::default();
        refresh.now();
        refresh.wait().await;

        let start = tokio::time::Instant::now();
        refresh.wait().await;

        assert!(
            start.elapsed() >= POLL_INTERVAL,
            "the second wait must fall back to the interval"
        );
    }
}
