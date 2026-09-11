//! Following one connection: whether the far machine is here, and when the
//! connection is over.
//!
//! modelpipe gives up on nothing. On the connect side [`PipeStatus::Idle`]
//! means the peer is not reachable *and is being dialled again*, with a
//! backoff, for as long as the pipe is held; [`PipeStatus::Closed`] is a
//! local decision — this side shut the pipe, or its own listener died —
//! and never the far machine's absence. So there is no policy for giving up
//! here. There was one — ninety seconds of `Idle` and the port was torn
//! down — and it was the thing that turned a closed laptop lid into a dead
//! port, and a different port the next morning. The port stays bound now,
//! whatever the far machine is doing, and answers `502` until it is back.
//!
//! What this file decides instead is what to *tell* people, and when to
//! nudge the transport. A peer away past [`AWAY_AFTER`] is announced as
//! away — so `gglib remote status` and the popover stop saying "connected"
//! over nothing — and announced back when it answers. While it is away,
//! modelpipe is told every [`NUDGE_EVERY`] that the network may have
//! changed, which is its cue to rebind a socket left on an interface that
//! no longer exists: a laptop that changed network while suspended is the
//! case it names. Both are policy, so [`follow`] takes the statuses and the
//! nudge through closures rather than reading a handle — a policy nothing
//! can exercise is a comment with a timer attached.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use modelpipe::{ConnectHandle, PipeStatus};
use tokio::sync::Mutex;
// `tokio::time`'s clock, not `std`'s: the deadlines below are tokio timers,
// and a clock the runtime cannot advance makes the dwell untestable — a
// paused-time test would restart it to the same instant every time and
// agree with whatever it was given.
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::connect::{DRAIN, LiveConnect};
use super::slot::Slot;

/// How long the pipe may sit `Idle` before the far machine is called away.
///
/// A re-dial that finds the peer straight back — a dropped packet, a relay
/// hiccup — is over inside modelpipe's first retry, and announcing it would
/// flap the status for nothing. Thirty seconds outlasts that and is still
/// short enough that a status read a minute after the lid closed says what
/// is true.
pub(super) const AWAY_AFTER: Duration = Duration::from_secs(30);

/// How often, while away, modelpipe is told the network may have changed.
///
/// It re-dials on its own; the nudge is for the socket underneath, which a
/// suspend can leave bound to an interface that is gone. Once a minute is
/// cheap and is about as long as a person waits before asking why.
pub(super) const NUDGE_EVERY: Duration = Duration::from_secs(60);

/// Why following a connection stopped.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Over {
    /// A teardown said so; the connection is somebody else's to clean up.
    Cancelled,
    /// modelpipe's own verdict, and terminal: this side closed it.
    Closed,
}

/// What the watcher tells the rest of the daemon about the far machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Presence {
    /// Idle past [`AWAY_AFTER`]; still being dialled.
    Away,
    /// Answering again.
    Here,
}

/// Follow the connection until it is over, announcing the far machine's
/// comings and goings on the way; then clear the slot and say so — unless
/// a newer connection has taken its place, or `disconnect` already did.
///
/// `away_since` is the clock `status` reads: unix milliseconds of the
/// moment the far machine was called away, or negative while it is here.
pub(super) async fn watch(
    live: Arc<Mutex<Slot<LiveConnect>>>,
    handle: Arc<ConnectHandle>,
    generation: u64,
    emitter: Arc<dyn AppEventEmitter>,
    cancel: CancellationToken,
    away_since: Arc<AtomicI64>,
    port: u16,
) {
    let over = follow(
        handle.status(),
        || handle.status_changed(),
        || handle.notify_network_change(),
        &cancel,
        |presence| match presence {
            Presence::Away => {
                away_since.store(unix_ms(), Ordering::Relaxed);
                warn!(
                    port,
                    "the remote is away; the port stays bound and it is being dialled"
                );
                emitter.emit(AppEvent::remote_away(port));
            }
            Presence::Here => {
                away_since.store(-1, Ordering::Relaxed);
                info!(port, "the remote is back");
                emitter.emit(AppEvent::remote_back(port));
            }
        },
    )
    .await;
    conclude(
        &live,
        |live| live.generation() == generation,
        over,
        || async {
            // Idempotent on the closed path and costs nothing; kept so a
            // watcher that concludes always leaves the port released.
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
    warn!("the remote connection closed; `gglib remote join` to dial again");
    emitter.emit(AppEvent::remote_disconnected());
    true
}

/// Wait out a connection, given where it starts, a way to ask for its next
/// status, and a way to nudge the transport; report the far machine's
/// presence as it changes.
async fn follow<F, Fut, N, NFut>(
    initial: PipeStatus,
    next: F,
    nudge: N,
    cancel: &CancellationToken,
    mut report: impl FnMut(Presence),
) -> Over
where
    F: Fn() -> Fut,
    Fut: Future<Output = PipeStatus>,
    N: Fn() -> NFut,
    NFut: Future<Output = ()>,
{
    // Read before waiting: `status_changed` snapshots at the moment it is
    // polled, so a pipe that went idle between the install and the first
    // call has nothing left to report and the clock would never start.
    let mut idle_since = idle_clock(None, initial);
    let mut away = false;
    let mut next_nudge: Option<Instant> = None;
    loop {
        // One deadline at a time: the away threshold while the pipe is idle
        // and not yet called away, the next nudge while it is away, nothing
        // while the far machine is here. `pending()` is the arm that says
        // "there is no deadline right now" without a timer to cancel.
        let deadline = async {
            match (away, idle_since, next_nudge) {
                (false, Some(since), _) => tokio::time::sleep_until(since + AWAY_AFTER).await,
                (true, _, Some(at)) => tokio::time::sleep_until(at).await,
                _ => std::future::pending().await,
            }
        };
        tokio::select! {
            () = cancel.cancelled() => return Over::Cancelled,
            () = deadline => {
                if !away {
                    away = true;
                    report(Presence::Away);
                }
                nudge().await;
                next_nudge = Some(Instant::now() + NUDGE_EVERY);
            }
            status = next() => {
                info!(path = status.as_str(), "remote connection path changed");
                if status == PipeStatus::Closed {
                    return Over::Closed;
                }
                idle_since = idle_clock(idle_since, status);
                if idle_since.is_none() && away {
                    away = false;
                    next_nudge = None;
                    report(Presence::Here);
                }
            }
        }
    }
}

/// The idle clock after `status`: `None` while the pipe is reaching the
/// peer, `Some(when it first went idle)` while it is not.
///
/// Only the *first* `Idle` starts it. Restarting on every report would let
/// a peer that goes from idle to idle — which is what a re-dial that finds
/// nobody looks like from here — put off being called away for ever.
fn idle_clock(current: Option<Instant>, status: PipeStatus) -> Option<Instant> {
    match status {
        PipeStatus::Idle => current.or_else(|| Some(Instant::now())),
        _ => None,
    }
}

/// Now, as unix milliseconds — what `away_since` stores and `status`
/// subtracts from.
pub(super) fn unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "connect_watch_tests.rs"]
mod connect_watch_tests;

#[cfg(test)]
#[path = "connect_teardown_tests.rs"]
mod connect_teardown_tests;
