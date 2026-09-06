//! Tests for when a connection is over.
//!
//! On a paused clock, so ninety seconds of grace cost nothing: tokio
//! advances time itself once every task is waiting, which is exactly the
//! shape of this loop — one deadline and one status to wait for.
//!
//! The statuses arrive through a channel rather than from a real
//! `ConnectHandle`, which would need an iroh endpoint and a peer to take
//! away, and the teardown through another closure over a slot of plain
//! `u64`s. That is why [`super::follow`] and [`super::conclude`] take both:
//! a policy nothing can drive is a comment with a timer attached, and the
//! whole of this file would otherwise be reachable only from a two-machine
//! run.

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::*;

/// The statuses a test hands to `follow`, and the handle it sends them on.
type Source = Mutex<UnboundedReceiver<PipeStatus>>;

fn source() -> (UnboundedSender<PipeStatus>, Source) {
    let (tx, rx) = unbounded_channel();
    (tx, Mutex::new(rx))
}

/// The next status, or nothing ever again.
///
/// A closed channel is a pipe that has stopped changing, which is what
/// being away *is* — so this waits for ever rather than panicking, and
/// leaves the dwell deadline as the only thing that can still fire.
async fn next_from(source: &Source) -> PipeStatus {
    match source.lock().await.recv().await {
        Some(status) => status,
        None => std::future::pending().await,
    }
}

/// Slack on a deadline assertion: enough for the loop's own wakeups, far
/// less than the grace it is asserting about.
const SLACK: Duration = Duration::from_secs(1);

/// **Every call to [`super::follow`] here is wrapped in a timeout**, even
/// the ones whose claim is not about time. `follow` has two arms that never
/// resolve on their own — a status source with nothing left to say, and the
/// `pending()` that stands in for "no deadline right now" — so a regression
/// that stops the dwell from arming, or drops the `Closed` arm, does not
/// make these tests fail. It makes them *hang*, and a hung test takes the
/// whole CI job's budget with it rather than naming the line that broke.
/// The paused clock makes the wrapper free.
const RUNAWAY: Duration = IDLE_GRACE.saturating_mul(3);

/// A peer that goes away and stays away is given up on.
///
/// The defect: nothing here ever acted on `Idle`, so a laptop that closed
/// its lid left `gglib remote status` saying "Connected" and every request
/// answered 502 — measured at twenty minutes on the serve side, and
/// unbounded on this one.
#[tokio::test(start_paused = true)]
async fn a_peer_that_stays_away_past_the_grace_is_given_up_on() {
    let (tx, rx) = source();
    tx.send(PipeStatus::Idle).expect("the loop is listening");
    drop(tx);

    let over = tokio::time::timeout(
        RUNAWAY,
        follow(
            PipeStatus::Direct,
            || next_from(&rx),
            &CancellationToken::new(),
        ),
    )
    .await
    .expect("the dwell never fired, so the peer is still being waited on");

    assert_eq!(over, Over::Unreachable);
}

/// A peer that comes back inside the grace is not.
///
/// This is why the policy is a dwell and not a break on the first `Idle`:
/// `Idle` is what modelpipe publishes *while* it re-dials, and tearing the
/// connection down on it would take away a pipe that was healing itself.
#[tokio::test(start_paused = true)]
async fn a_peer_that_comes_back_inside_the_grace_keeps_its_connection() {
    let (tx, rx) = source();
    tokio::spawn(async move {
        tx.send(PipeStatus::Idle).expect("the loop is listening");
        tokio::time::sleep(IDLE_GRACE / 2).await;
        tx.send(PipeStatus::Direct).expect("the loop is listening");
        // Held open, so the loop waits on a status that never comes rather
        // than on a closed channel — a peer that is simply connected.
        tokio::time::sleep(IDLE_GRACE * 10).await;
    });

    // The one place the timeout is the assertion rather than a guard
    // against a hang: outliving it is what "kept its connection" means.
    let waited = tokio::time::timeout(
        RUNAWAY,
        follow(
            PipeStatus::Direct,
            || next_from(&rx),
            &CancellationToken::new(),
        ),
    )
    .await;

    assert!(
        waited.is_err(),
        "the connection was given up on despite the peer coming back: {waited:?}"
    );
}

/// A second `Idle` does not restart the clock.
///
/// A re-dial that finds nobody publishes `Idle` again, and there is no
/// bound on how many times it can: a clock that restarted on each one
/// would never expire, which is the same defect wearing a timer.
#[tokio::test(start_paused = true)]
async fn a_second_idle_report_does_not_buy_the_peer_more_time() {
    let (tx, rx) = source();
    tokio::spawn(async move {
        tx.send(PipeStatus::Idle).expect("the loop is listening");
        tokio::time::sleep(IDLE_GRACE / 2).await;
        tx.send(PipeStatus::Idle).expect("the loop is listening");
        tokio::time::sleep(IDLE_GRACE * 10).await;
    });

    let over = tokio::time::timeout(
        IDLE_GRACE + SLACK,
        follow(
            PipeStatus::Direct,
            || next_from(&rx),
            &CancellationToken::new(),
        ),
    )
    .await
    .expect("the deadline is the first idle's, not the last one's");

    assert_eq!(over, Over::Unreachable);
}

/// A pipe that is already idle when the watcher starts is on the clock from
/// that moment.
///
/// `status_changed` snapshots when it is polled, so a pipe that went idle
/// between the install and the first call has nothing left to report — and
/// a loop that only ever waited would never start the clock at all.
#[tokio::test(start_paused = true)]
async fn a_pipe_that_is_already_idle_is_on_the_clock_from_the_start() {
    let (tx, rx) = source();
    drop(tx);

    let over = tokio::time::timeout(
        IDLE_GRACE + SLACK,
        follow(
            PipeStatus::Idle,
            || next_from(&rx),
            &CancellationToken::new(),
        ),
    )
    .await
    .expect("nothing started the clock");

    assert_eq!(over, Over::Unreachable);
}

/// modelpipe's own verdict ends it at once, with no grace involved.
///
/// The deadline is half the grace on purpose: it is the "at once" half of
/// the claim. A loop that dropped the `Closed` arm and let the dwell decide
/// would still return `Unreachable` eventually, and only a deadline shorter
/// than the grace can tell that apart from this.
#[tokio::test(start_paused = true)]
async fn a_closed_pipe_is_over_immediately() {
    let (tx, rx) = source();
    tx.send(PipeStatus::Closed).expect("the loop is listening");

    let over = tokio::time::timeout(
        IDLE_GRACE / 2,
        follow(
            PipeStatus::Relayed,
            || next_from(&rx),
            &CancellationToken::new(),
        ),
    )
    .await
    .expect("modelpipe's own verdict was left to the dwell to rediscover");

    assert_eq!(over, Over::Closed);
}

/// A cancelled watcher says it was cancelled, which is what stops it
/// clearing a slot and announcing a disconnection `disconnect` has already
/// announced.
#[tokio::test(start_paused = true)]
async fn a_cancelled_watcher_leaves_the_teardown_to_whoever_cancelled_it() {
    let (tx, rx) = source();
    tx.send(PipeStatus::Idle).expect("the loop is listening");
    let cancel = CancellationToken::new();
    cancel.cancel();

    let over = tokio::time::timeout(
        IDLE_GRACE / 2,
        follow(PipeStatus::Direct, || next_from(&rx), &cancel),
    )
    .await
    .expect("a cancelled watcher was left waiting on the peer it was told to stop watching");

    assert_eq!(over, Over::Cancelled);
}
