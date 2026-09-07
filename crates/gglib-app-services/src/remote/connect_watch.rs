//! Following one connection until it is over, and deciding when that is.
//!
//! `Closed` is modelpipe's answer and needs no policy. `Idle` is not an
//! answer: on the connect side it means the peer went away and
//! `peer::keep_connected` is looking for it again, and modelpipe says
//! plainly that it cannot tell a sleeping laptop from a dead one and leaves
//! the policy to whoever is watching. This file is that policy — which is
//! why [`follow`] takes the statuses through a closure rather than reading
//! the handle itself: the timing *is* the policy, and a policy nothing can
//! exercise is a comment.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use modelpipe::{ConnectHandle, PipeStatus};
use tokio::sync::Mutex;
// `tokio::time`'s clock, not `std`'s: the deadline below is a tokio timer,
// and a clock the runtime cannot advance makes the dwell untestable — a
// paused-time test would restart it to the same instant every time and
// agree with whatever it was given.
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::connect::{DRAIN, LiveConnect};
use super::slot::Slot;

/// How long the pipe may sit `Idle` before this side gives up on it.
///
/// It has to outlast modelpipe's own patience or it would tear down a pipe
/// that was about to heal: its re-dial backoff tops out at thirty seconds,
/// and a dial at a machine that is simply switched off takes iroh about
/// thirty more to give up on, so one full attempt-plus-wait is around a
/// minute. Ninety seconds clears that with room, and is short enough that
/// `gglib remote status` stops calling a machine that has been off for two
/// minutes "Connected" — which is what it did for ever, because nothing
/// here ever acted on `Idle` at all.
const IDLE_GRACE: Duration = Duration::from_secs(90);

/// Why following a connection stopped.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Over {
    /// A teardown said so; the connection is somebody else's to clean up.
    Cancelled,
    /// modelpipe's own verdict, and terminal.
    Closed,
    /// The peer has been away longer than [`IDLE_GRACE`]. This side's
    /// verdict, not modelpipe's.
    Unreachable,
}

/// Follow the connection until it is over, then clear it and say so —
/// unless a newer connection has taken its place, or `disconnect` already
/// did.
pub(super) async fn watch(
    live: Arc<Mutex<Slot<LiveConnect>>>,
    handle: Arc<ConnectHandle>,
    generation: u64,
    emitter: Arc<dyn AppEventEmitter>,
    cancel: CancellationToken,
) {
    let over = follow(handle.status(), || handle.status_changed(), &cancel).await;
    conclude(
        &live,
        |live| live.generation() == generation,
        over,
        || async {
            // The local port is this side's, and on the unreachable path
            // modelpipe is still re-dialling behind it. Leaving it up would
            // mean a bound port answering 502 for a machine nobody is
            // waiting for any more; on the closed path this is idempotent
            // and costs nothing.
            handle.shutdown_timeout(DRAIN).await;
        },
        &*emitter,
    )
    .await;
}

/// Take the connection down and announce it, once following is over.
///
/// Generic over what the slot holds, for the reason [`follow`] takes its
/// statuses through a closure: a [`LiveConnect`] carries an
/// `Arc<ConnectHandle>`, which needs an iroh endpoint and a peer that
/// answers, so anything written to be reachable only through one is
/// unchecked. What that leaves here is the part with the decisions in it —
/// that a cancelled watcher touches nothing, that a watcher whose
/// connection was replaced takes down neither the replacement nor a dial on
/// its way to becoming one, and that the port is released *before* the loss
/// is announced rather than left bound behind a GUI that has already
/// redrawn.
async fn conclude<T, Fut>(
    live: &Mutex<Slot<T>>,
    is_mine: impl FnOnce(&T) -> bool,
    over: Over,
    shutdown: impl FnOnce() -> Fut,
    emitter: &dyn AppEventEmitter,
) -> bool
where
    Fut: Future<Output = ()>,
{
    // Somebody else is taking this down and announcing it. Two
    // `RemoteDisconnected` for one connection is worse than a silent
    // watcher — a GUI counts them — and the handle being drained here is
    // one `disconnect` is already draining.
    if over == Over::Cancelled {
        return false;
    }
    if live.lock().await.take_if(is_mine).is_none() {
        return false;
    }
    shutdown().await;
    match over {
        Over::Unreachable => warn!(
            grace_s = IDLE_GRACE.as_secs(),
            "the remote has been unreachable past the grace; `gglib remote connect` to dial again"
        ),
        _ => warn!("the remote connection closed; `gglib remote connect` to dial again"),
    }
    emitter.emit(AppEvent::remote_disconnected());
    true
}

/// Wait out a connection, given where it starts and a way to ask for its
/// next status.
async fn follow<F, Fut>(initial: PipeStatus, next: F, cancel: &CancellationToken) -> Over
where
    F: Fn() -> Fut,
    Fut: Future<Output = PipeStatus>,
{
    // Read before waiting: `status_changed` snapshots at the moment it is
    // polled, so a pipe that went idle between the install and the first
    // call has nothing left to report and the clock would never start.
    let mut idle_since = idle_clock(None, initial);
    loop {
        // Armed only while the pipe is idle. `pending()` is the arm that
        // says "there is no deadline right now" without a timer to cancel.
        let dwell = async {
            match idle_since {
                Some(since) => tokio::time::sleep_until(since + IDLE_GRACE).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            () = cancel.cancelled() => return Over::Cancelled,
            () = dwell => return Over::Unreachable,
            status = next() => {
                info!(path = status.as_str(), "remote connection path changed");
                if status == PipeStatus::Closed {
                    return Over::Closed;
                }
                idle_since = idle_clock(idle_since, status);
            }
        }
    }
}

/// The idle clock after `status`: `None` while the pipe is reaching the
/// peer, `Some(when it first went idle)` while it is not.
///
/// Only the *first* `Idle` starts it. Restarting on every report would let
/// a peer that goes from idle to idle — which is what a re-dial that finds
/// nobody looks like from here — hold a dead connection open for ever,
/// which is the state this whole policy exists to end.
fn idle_clock(current: Option<Instant>, status: PipeStatus) -> Option<Instant> {
    match status {
        PipeStatus::Idle => current.or_else(|| Some(Instant::now())),
        _ => None,
    }
}

#[cfg(test)]
#[path = "connect_watch_tests.rs"]
mod connect_watch_tests;

#[cfg(test)]
#[path = "connect_teardown_tests.rs"]
mod connect_teardown_tests;
