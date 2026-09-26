//! Tests for what the watcher tells people.
//!
//! Where the grace is *measured from* is the sibling's subject; this file
//! holds the harness both use, because duplicating a stand-in clock is how
//! two files come to disagree about what modelpipe does.
//!
//! On a paused clock, so thirty seconds of grace costs nothing: tokio
//! advances time itself once every task is waiting, which is exactly the
//! shape of this loop — one deadline and one status to wait for.
//!
//! The statuses arrive through a channel rather than from a real
//! `ConnectHandle`, which would need an iroh endpoint and a peer to take
//! away, and the reports are a channel too. That is why [`super::follow`]
//! takes both through closures: a policy nothing can drive is a comment
//! with a timer attached, and the whole of this file would otherwise be
//! reachable only from a two-machine run.
//!
//! [`Clock`] stands in for `ConnectHandle::idle_for`, and keeps modelpipe's
//! rule rather than a convenient one: the clock starts when the pipe goes
//! idle, is cleared by anything else, and *restarts* on an idle that
//! follows a reach. Holding it here rather than in `follow` is the point of
//! the change — a test can now move it without sending a status, which is
//! what a coalesced round trip looks like from the watcher's side.

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::Instant;

use super::*;

/// modelpipe's idle clock, stood up here: what
/// [`ConnectHandle::idle_for`] answers, on the rule modelpipe applies
/// inside its own status transition.
#[derive(Clone)]
pub(super) struct Clock(Arc<std::sync::Mutex<Option<Instant>>>);

impl Clock {
    pub(super) fn new() -> Self {
        Self(Arc::new(std::sync::Mutex::new(None)))
    }

    /// Reaching the peer: no idleness to report.
    pub(super) fn reached(&self) {
        *self.0.lock().expect("the clock is not poisoned") = None;
    }

    /// Idle: start the clock, or leave a running one where it is. Only the
    /// *first* idle of a run starts it, which is why a re-dial that finds
    /// nobody buys the peer no more time.
    pub(super) fn idle(&self) {
        let mut at = self.0.lock().expect("the clock is not poisoned");
        *at = at.or_else(|| Some(Instant::now()));
    }

    /// What `idle_for()` answers now.
    pub(super) fn read(&self) -> Option<Duration> {
        self.0
            .lock()
            .expect("the clock is not poisoned")
            .map(|since| since.elapsed())
    }
}

/// The statuses a test hands to `follow`, and the handle it sends them on.
pub(super) type Source = Mutex<UnboundedReceiver<PipeStatus>>;

pub(super) fn source() -> (UnboundedSender<PipeStatus>, Source) {
    let (tx, rx) = unbounded_channel();
    (tx, Mutex::new(rx))
}

/// The next status, or nothing ever again — moving the clock the way
/// modelpipe moves it when it sets that status.
///
/// A closed channel is a pipe that has stopped changing, which is what
/// being away *is* — so this waits for ever rather than panicking, and
/// leaves the deadline as the only thing that can still fire.
async fn next_from(source: &Source, clock: &Clock) -> PipeStatus {
    match source.lock().await.recv().await {
        Some(status) => {
            match status {
                PipeStatus::Idle => clock.idle(),
                _ => clock.reached(),
            }
            status
        }
        None => std::future::pending().await,
    }
}

/// Slack on a deadline assertion: enough for the loop's own wakeups, far
/// less than the grace it is asserting about.
pub(super) const SLACK: Duration = Duration::from_secs(1);

/// **Every call to [`super::follow`] here is wrapped in a timeout**, even
/// the ones whose claim is not about time. `follow` has arms that never
/// resolve on their own, so a regression that drops the `Closed` arm does
/// not make these tests fail — it makes them *hang*, and a hung test takes
/// the whole CI job's budget with it rather than naming the line that
/// broke. The paused clock makes the wrapper free.
pub(super) const RUNAWAY: Duration = Duration::from_mins(10);

/// A `follow` under test: its reports and its outcome once it ends. A test
/// that needs to move the clock keeps its own handle to it — the point of
/// [`follow_with`] — so this does not hold one.
pub(super) struct Followed {
    pub(super) reports: UnboundedReceiver<Presence>,
    pub(super) over: tokio::task::JoinHandle<Over>,
    pub(super) cancel: CancellationToken,
}

/// Start `follow` over `source`, with the pipe reaching the peer.
pub(super) fn follow_from(source: Source) -> Followed {
    follow_with(source, &Clock::new())
}

/// Start `follow` over `source` with `clock` already where the test wants
/// it — which is how a pipe that was idle before the watcher existed is
/// set up, since modelpipe's clock does not wait to be noticed.
pub(super) fn follow_with(source: Source, clock: &Clock) -> Followed {
    let (report_tx, reports) = unbounded_channel();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let read = clock.clone();
    let moved = clock.clone();
    let over = tokio::spawn(async move {
        follow(
            || next_from(&source, &moved),
            || read.read(),
            &token,
            |presence| {
                let _ = report_tx.send(presence);
            },
        )
        .await
    });
    Followed {
        reports,
        over,
        cancel,
    }
}

/// The next report inside `within`, or `None` when nothing was said.
pub(super) async fn said(followed: &mut Followed, within: Duration) -> Option<Presence> {
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
    let mut followed = follow_from(source);
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
    let mut followed = follow_from(source);
    tx.send(PipeStatus::Idle).unwrap();
    tokio::time::sleep(AWAY_AFTER / 2).await;
    tx.send(PipeStatus::Relayed).unwrap();

    assert_eq!(said(&mut followed, AWAY_AFTER * 2).await, None);

    // And the watcher is still alive and still re-arms — an absence on its
    // own would also be satisfied by a `follow` that announced nothing ever.
    tx.send(PipeStatus::Idle).unwrap();
    assert_eq!(
        said(&mut followed, AWAY_AFTER + SLACK).await,
        Some(Presence::Away),
        "silence during the blip was the grace, not a dead watcher"
    );
    followed.cancel.cancel();
}

/// Only the first `Idle` starts the clock: a re-dial that finds nobody
/// reports idle again, and that must not buy the peer more time before it
/// is called away.
#[tokio::test(start_paused = true)]
async fn a_second_idle_report_does_not_put_off_being_called_away() {
    let (tx, source) = source();
    let mut followed = follow_from(source);
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

/// A pipe that closes while the peer is away says nothing on the way out.
///
/// `Closed` has to be answered *before* the presence logic, not after. It is
/// not `Idle`, so a check that ran first would read it as the far machine
/// answering, announce `Here`, and emit `remote_back` for a connection that
/// is ending — the popover would flick to connected as the port went away.
/// The clock cannot help: `idle_for()` answers `None` for a closed pipe
/// exactly as it does for a reached one, so the status is the only thing
/// that tells them apart.
#[tokio::test(start_paused = true)]
async fn a_pipe_that_closes_while_away_announces_nothing_on_the_way_out() {
    let (tx, source) = source();
    let clock = Clock::new();
    clock.idle();
    let mut followed = follow_with(source, &clock);
    assert_eq!(
        said(&mut followed, AWAY_AFTER + SLACK).await,
        Some(Presence::Away)
    );

    clock.reached();
    tx.send(PipeStatus::Closed).unwrap();
    assert_eq!(
        tokio::time::timeout(RUNAWAY, followed.over)
            .await
            .unwrap()
            .unwrap(),
        Over::Closed
    );
    assert_eq!(
        followed.reports.try_recv().ok(),
        None,
        "a closing pipe is not the far machine coming back"
    );
}

/// `Closed` is modelpipe's verdict and needs no grace: this side shut the
/// pipe, or its listener died, and either way there is nothing to wait for.
#[tokio::test(start_paused = true)]
async fn a_closed_pipe_is_over_immediately() {
    let (tx, source) = source();
    let followed = follow_from(source);
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
    let followed = follow_from(source);
    followed.cancel.cancel();
    assert_eq!(
        tokio::time::timeout(RUNAWAY, followed.over)
            .await
            .unwrap()
            .unwrap(),
        Over::Cancelled
    );
}
