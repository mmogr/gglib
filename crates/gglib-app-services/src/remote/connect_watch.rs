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
//! What this file decides instead is what to *tell* people. A peer away
//! past [`AWAY_AFTER`] is announced as away — so `gglib remote status` and
//! the popover stop saying "connected" over nothing — and announced back
//! when it answers. That is the whole policy here now: the nudge that used
//! to live beside it is modelpipe's, and the clock it ran on is
//! [`ConnectHandle::idle_for`], read afresh on every turn rather than kept
//! here.
//!
//! [`follow`] still takes the statuses and the clock through closures
//! rather than reading a handle. A `ConnectHandle` needs an iroh endpoint
//! and a peer to take away, so a policy that reached for one would be
//! exercisable only from a two-machine run — a comment with a timer
//! attached.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use modelpipe::{ConnectHandle, PipeStatus};
use tokio::sync::Mutex;
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
        || handle.status_changed(),
        || handle.idle_for(),
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

/// Wait out a connection, given a way to ask for its next status and a way
/// to ask how long it has been idle; report the far machine's presence as
/// it changes.
///
/// `idle` is [`ConnectHandle::idle_for`]: `None` while the pipe is reaching
/// the peer, `Some(how long)` while it is not. It needs no seeding from a
/// starting status, because modelpipe starts that clock when the pipe goes
/// idle rather than when this watcher first looks — a pipe already idle at
/// the install is on the clock, not waiting to be noticed.
async fn follow<F, Fut, I>(
    next: F,
    idle: I,
    cancel: &CancellationToken,
    mut report: impl FnMut(Presence),
) -> Over
where
    F: Fn() -> Fut,
    Fut: Future<Output = PipeStatus>,
    I: Fn() -> Option<Duration>,
{
    let mut away = false;
    loop {
        // One deadline at a time: what is left of the grace while the pipe
        // is idle and not yet called away, and nothing otherwise — once the
        // peer is away there is no timer here at all, because the nudge
        // that used to keep one is modelpipe's now. `pending()` is the arm
        // that says "no deadline right now" without a timer to cancel.
        //
        // `saturating_sub` because the grace can already be spent when this
        // is reached — `Duration`'s `Sub` panics on underflow — in which
        // case the sleep is zero and the arm fires at once.
        //
        // That zero sleep is safe because of two things below, not one: this
        // `away` guard, which stops it recurring once the peer is announced
        // away, and `>=` rather than `>` in the arm, which stops it
        // recurring before. Weaken `>` and the loop spins without advancing
        // a paused clock, which *hangs* the suite rather than failing it: no
        // `tokio::time::timeout` can bound a busy task. Dropping the `away`
        // guard does both — it also queues a second `Away`, which the
        // closing-pipe test catches.
        let remaining = if away {
            None
        } else {
            idle().map(|spent| AWAY_AFTER.saturating_sub(spent))
        };
        let deadline = async {
            match remaining {
                Some(left) => tokio::time::sleep(left).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            () = cancel.cancelled() => return Over::Cancelled,
            () = deadline => {
                // Read again rather than trust the sleep. `status_changed`
                // coalesces, so the peer can have been reached and lost
                // again while this was waiting, with no status arriving to
                // say so; modelpipe's clock restarted, and the grace this
                // slept out belongs to an idleness that ended. Anything
                // short of the threshold re-arms on the way round.
                match idle() {
                    Some(spent) if spent >= AWAY_AFTER => {
                        away = true;
                        report(Presence::Away);
                    }
                    _ => {}
                }
            }
            status = next() => {
                info!(path = status.as_str(), "remote connection path changed");
                // Before any reading of the clock, because `idle_for`
                // answers `None` for a closed pipe exactly as it does for a
                // reached one. The status is the only thing that tells them
                // apart, so this must stay above everything below it.
                if status == PipeStatus::Closed {
                    return Over::Closed;
                }
                // `!= Idle` reads as belt and braces and is: while `away`
                // is true the parked `status_changed` cannot answer `Idle`,
                // because modelpipe's `set_status` no-ops on a repeat. It
                // stays because "the peer answered" is the condition being
                // named, and naming it by what it is survives an upstream
                // that starts republishing.
                if away && status != PipeStatus::Idle {
                    away = false;
                    report(Presence::Here);
                }
            }
        }
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
#[path = "connect_idle_tests.rs"]
mod connect_idle_tests;

#[cfg(test)]
#[path = "connect_teardown_tests.rs"]
mod connect_teardown_tests;
