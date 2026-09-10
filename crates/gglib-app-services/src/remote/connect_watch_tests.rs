//! Tests for what the watcher tells people, and when it nudges.
//!
//! On a paused clock, so thirty seconds of grace and a minute between
//! nudges cost nothing: tokio advances time itself once every task is
//! waiting, which is exactly the shape of this loop — one deadline and one
//! status to wait for.
//!
//! The statuses arrive through a channel rather than from a real
//! `ConnectHandle`, which would need an iroh endpoint and a peer to take
//! away; the nudge is a counter; the reports are a channel too. That is why
//! [`super::follow`] takes all three through closures: a policy nothing can
//! drive is a comment with a timer attached, and the whole of this file
//! would otherwise be reachable only from a two-machine run.

use std::sync::atomic::{AtomicUsize, Ordering};

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
/// leaves the deadlines as the only things that can still fire.
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
/// the ones whose claim is not about time. `follow` has arms that never
/// resolve on their own, so a regression that drops the `Closed` arm does
/// not make these tests fail — it makes them *hang*, and a hung test takes
/// the whole CI job's budget with it rather than naming the line that
/// broke. The paused clock makes the wrapper free.
const RUNAWAY: Duration = Duration::from_secs(600);

/// A `follow` under test: its reports, its nudge count, and its outcome
/// once it ends.
struct Followed {
    reports: UnboundedReceiver<Presence>,
    nudges: Arc<AtomicUsize>,
    over: tokio::task::JoinHandle<Over>,
    cancel: CancellationToken,
}

fn follow_from(initial: PipeStatus, source: Source) -> Followed {
    let (report_tx, reports) = unbounded_channel();
    let nudges = Arc::new(AtomicUsize::new(0));
    let cancel = CancellationToken::new();
    let counted = Arc::clone(&nudges);
    let token = cancel.clone();
    let over = tokio::spawn(async move {
        follow(
            initial,
            || next_from(&source),
            || {
                let counted = Arc::clone(&counted);
                async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                }
            },
            &token,
            |presence| {
                let _ = report_tx.send(presence);
            },
        )
        .await
    });
    Followed {
        reports,
        nudges,
        over,
        cancel,
    }
}

/// The next report inside `within`, or `None` when nothing was said.
async fn said(followed: &mut Followed, within: Duration) -> Option<Presence> {
    tokio::time::timeout(within, followed.reports.recv())
        .await
        .ok()
        .flatten()
}

/// A peer that goes away and stays away is called away — and the
/// connection is *kept*, which is the whole change: nothing here ends.
///
/// The defect this replaces: ninety seconds of `Idle` tore the port down,
/// so a closed laptop lid was a dead port, and a different port next time.
#[tokio::test(start_paused = true)]
async fn a_peer_away_past_the_grace_is_called_away_and_the_connection_is_kept() {
    let (tx, source) = source();
    let mut followed = follow_from(PipeStatus::Direct, source);
    tx.send(PipeStatus::Idle).unwrap();

    assert_eq!(
        said(&mut followed, AWAY_AFTER + SLACK).await,
        Some(Presence::Away),
        "called away once the grace has passed"
    );
    tokio::time::sleep(AWAY_AFTER * 4).await;
    assert!(
        !followed.over.is_finished(),
        "still following: being away is not being over"
    );

    tx.send(PipeStatus::Direct).unwrap();
    assert_eq!(
        said(&mut followed, SLACK).await,
        Some(Presence::Here),
        "and called back when the far machine answers"
    );
    followed.cancel.cancel();
    assert_eq!(
        tokio::time::timeout(RUNAWAY, followed.over)
            .await
            .unwrap()
            .unwrap(),
        Over::Cancelled
    );
}

/// A blip — idle, then back inside the grace — says nothing. Announcing
/// every re-dial that finds the peer straight back would flap the status
/// for nothing.
#[tokio::test(start_paused = true)]
async fn a_blip_inside_the_grace_says_nothing() {
    let (tx, source) = source();
    let mut followed = follow_from(PipeStatus::Direct, source);
    tx.send(PipeStatus::Idle).unwrap();
    tokio::time::sleep(AWAY_AFTER / 2).await;
    tx.send(PipeStatus::Relayed).unwrap();

    assert_eq!(said(&mut followed, AWAY_AFTER * 2).await, None);
    assert_eq!(followed.nudges.load(Ordering::SeqCst), 0, "nor nudged");
    followed.cancel.cancel();
}

/// Only the first `Idle` starts the clock: a re-dial that finds nobody
/// reports idle again, and that must not buy the peer more time before it
/// is called away.
#[tokio::test(start_paused = true)]
async fn a_second_idle_report_does_not_put_off_being_called_away() {
    let (tx, source) = source();
    let mut followed = follow_from(PipeStatus::Direct, source);
    tx.send(PipeStatus::Idle).unwrap();
    tokio::time::sleep(AWAY_AFTER / 2).await;
    tx.send(PipeStatus::Idle).unwrap();

    assert_eq!(
        said(&mut followed, AWAY_AFTER / 2 + SLACK).await,
        Some(Presence::Away),
        "called away at the grace from the *first* idle"
    );
    followed.cancel.cancel();
}

/// While the far machine is away, the transport is told the network may
/// have changed — once on being called away, then every minute — because a
/// suspend can leave the dialling socket on an interface that is gone, and
/// that is the one case modelpipe's own re-dial cannot fix from inside.
#[tokio::test(start_paused = true)]
async fn while_away_the_transport_is_nudged_on_arrival_and_every_minute() {
    let (tx, source) = source();
    let mut followed = follow_from(PipeStatus::Idle, source);

    assert_eq!(
        said(&mut followed, AWAY_AFTER + SLACK).await,
        Some(Presence::Away)
    );
    tokio::time::sleep(SLACK).await;
    assert_eq!(
        followed.nudges.load(Ordering::SeqCst),
        1,
        "nudged on being called away"
    );
    tokio::time::sleep(NUDGE_EVERY).await;
    assert_eq!(
        followed.nudges.load(Ordering::SeqCst),
        2,
        "and again a minute later"
    );
    tokio::time::sleep(NUDGE_EVERY).await;
    assert_eq!(followed.nudges.load(Ordering::SeqCst), 3);

    tx.send(PipeStatus::Direct).unwrap();
    assert_eq!(said(&mut followed, SLACK).await, Some(Presence::Here));
    tokio::time::sleep(NUDGE_EVERY * 3).await;
    assert_eq!(
        followed.nudges.load(Ordering::SeqCst),
        3,
        "nothing to nudge about once it is back"
    );
    followed.cancel.cancel();
}

/// A pipe that is already idle when the watcher starts is on the clock from
/// the start: the first `status_changed` would otherwise wait for a change
/// that never comes.
#[tokio::test(start_paused = true)]
async fn a_pipe_that_is_already_idle_is_on_the_clock_from_the_start() {
    let (_tx, source) = source();
    let mut followed = follow_from(PipeStatus::Idle, source);
    assert_eq!(
        said(&mut followed, AWAY_AFTER + SLACK).await,
        Some(Presence::Away)
    );
    followed.cancel.cancel();
}

/// `Closed` is modelpipe's verdict and needs no grace: this side shut the
/// pipe, or its listener died, and either way there is nothing to wait for.
#[tokio::test(start_paused = true)]
async fn a_closed_pipe_is_over_immediately() {
    let (tx, source) = source();
    let followed = follow_from(PipeStatus::Direct, source);
    tx.send(PipeStatus::Closed).unwrap();
    assert_eq!(
        tokio::time::timeout(RUNAWAY, followed.over)
            .await
            .unwrap()
            .unwrap(),
        Over::Closed
    );
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_watcher_leaves_the_teardown_to_whoever_cancelled_it() {
    let (_tx, source) = source();
    let followed = follow_from(PipeStatus::Idle, source);
    followed.cancel.cancel();
    assert_eq!(
        tokio::time::timeout(RUNAWAY, followed.over)
            .await
            .unwrap()
            .unwrap(),
        Over::Cancelled
    );
}
