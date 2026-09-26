//! Tests for the grace itself: where it is measured from now that the clock
//! is modelpipe's rather than this watcher's, how long it is, and when it
//! re-arms.
//!
//! A sibling of `connect_watch_tests.rs`, which was 255 lines against a
//! 300-line budget and would have gone over with these, and because they are
//! a different question about the same loop: that file
//! asks *what is announced*, this one asks *when* — and every case here turns
//! on a clock that was already running before `follow` looked at it, which is
//! precisely what [`ConnectHandle::idle_for`] gives and a clock kept here
//! could not.
//!
//! The harness — [`Clock`], [`follow_with`], [`said`] — comes from the
//! sibling rather than being repeated, so both files agree by construction
//! about the rule they are standing in for.

use super::connect_watch_tests::{Clock, SLACK, follow_from, follow_with, said, source};
use super::*;

/// A pipe that is already idle when the watcher starts is on the clock from
/// the start: the first `status_changed` would otherwise wait for a change
/// that never comes.
///
/// `follow` seeds nothing to get this. modelpipe's clock was started when
/// the pipe went idle, which is before this watcher existed, so the first
/// reading already carries the idleness — the case that used to need an
/// `initial` status handed in.
#[tokio::test(start_paused = true)]
async fn a_pipe_that_is_already_idle_is_on_the_clock_from_the_start() {
    let (_tx, source) = source();
    let clock = Clock::new();
    clock.idle();
    let mut followed = follow_with(source, &clock);
    assert_eq!(
        said(&mut followed, AWAY_AFTER + SLACK).await,
        Some(Presence::Away)
    );
    followed.cancel.cancel();
}

/// Half the grace spent before the watcher looks, and the peer is called
/// away at the grace from when it *went* idle — not from when this side
/// first read the clock.
#[tokio::test(start_paused = true)]
async fn a_pipe_already_half_way_through_the_grace_is_called_away_on_time() {
    let (_tx, source) = source();
    let clock = Clock::new();
    clock.idle();
    tokio::time::sleep(AWAY_AFTER / 2).await;

    let mut followed = follow_with(source, &clock);
    assert_eq!(
        said(&mut followed, AWAY_AFTER / 2 + SLACK).await,
        Some(Presence::Away),
        "the remaining half, not a fresh whole"
    );
    followed.cancel.cancel();
}

/// The peer was reached and lost again while nothing was watching, so no
/// status arrived to say so — `status_changed` coalesces. modelpipe
/// restarted its clock; the grace this side was sleeping out belongs to an
/// idleness that ended, and the announcement must wait for the new one.
///
/// This is what reading the clock afresh buys. gglib's own clock could not
/// see a round trip it was never told about, so it called the peer away on
/// an anchor the pipe had already abandoned.
#[tokio::test(start_paused = true)]
#[allow(
    clippy::unchecked_time_subtraction,
    reason = "grandfathered at lint inheritance, #1157"
)]
async fn a_round_trip_nobody_saw_restarts_the_grace() {
    let (_tx, source) = source();
    let clock = Clock::new();
    clock.idle();
    let mut followed = follow_with(source, &clock);

    // Most of the way to being called away, then reached and lost again
    // with no status sent: exactly what gglib observes when two transitions
    // land between its polls.
    tokio::time::sleep(AWAY_AFTER - SLACK).await;
    clock.reached();
    clock.idle();

    assert_eq!(
        said(&mut followed, SLACK * 2).await,
        None,
        "the old anchor is gone, so nothing is said on its schedule"
    );
    assert_eq!(
        said(&mut followed, AWAY_AFTER + SLACK).await,
        Some(Presence::Away),
        "and away arrives a grace after the idleness that is actually running"
    );
    followed.cancel.cancel();
}

/// A grace already spent when the watcher first looks is announced at once,
/// not slept out again.
///
/// This is the one input on which `AWAY_AFTER.saturating_sub(spent)` differs
/// from `AWAY_AFTER - spent`: the latter panics on underflow, in release as
/// well as debug. It is reachable in production — `connect_dial` settles the
/// key and installs the slot between the dial returning and the watcher
/// being spawned, while modelpipe's clock has been running since the
/// transition — so without this the arithmetic that prevents a panic is
/// pinned by nothing.
#[tokio::test(start_paused = true)]
async fn a_grace_already_spent_is_announced_at_once() {
    let (_tx, source) = source();
    let clock = Clock::new();
    clock.idle();
    tokio::time::sleep(AWAY_AFTER * 2).await;

    let mut followed = follow_with(source, &clock);
    assert_eq!(
        said(&mut followed, SLACK).await,
        Some(Presence::Away),
        "a grace that is already over is not slept out a second time"
    );
    followed.cancel.cancel();
}

/// Coming back re-arms: a machine that goes away, returns, and goes away
/// again is announced away *both* times.
///
/// Without this, dropping `away = false` on the way back is invisible —
/// `remaining` stays `None` for ever and the second absence is never
/// announced, leaving `gglib remote status` saying connected over nothing
/// for the rest of the session.
#[tokio::test(start_paused = true)]
async fn a_machine_that_comes_back_and_goes_again_is_announced_both_times() {
    let (tx, source) = source();
    let mut followed = follow_from(source);

    tx.send(PipeStatus::Idle).unwrap();
    assert_eq!(
        said(&mut followed, AWAY_AFTER + SLACK).await,
        Some(Presence::Away)
    );
    tx.send(PipeStatus::Direct).unwrap();
    assert_eq!(said(&mut followed, SLACK).await, Some(Presence::Here));

    tx.send(PipeStatus::Idle).unwrap();
    assert_eq!(
        said(&mut followed, AWAY_AFTER + SLACK).await,
        Some(Presence::Away),
        "the second absence is announced too"
    );
    followed.cancel.cancel();
}

/// The grace is thirty seconds, and three pieces of prose sell that number:
/// `AWAY_AFTER`'s own doc, `remote/README.md` and `docs/remote.md`. Every
/// other test here says `AWAY_AFTER`, so the constant and the assertions
/// move together and the number itself is pinned by nothing.
#[test]
fn the_grace_is_the_thirty_seconds_the_documentation_promises() {
    assert_eq!(AWAY_AFTER, Duration::from_secs(30));
}
