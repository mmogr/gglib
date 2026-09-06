//! Tests for what happens once following a connection is over.
//!
//! A sibling of `connect_watch_tests.rs`, which is within a dozen lines of
//! the file-size budget, and because these are the other half of the
//! subject: that file asks *when* a connection is finished,
//! [`super::conclude`] is *what is then done about it* — the slot cleared,
//! the port dropped, the disconnection announced.
//!
//! The slot holds plain `u64`s here. `conclude` is generic for that reason
//! alone: production puts a `LiveConnect` in it, which carries an
//! `Arc<ConnectHandle>` and so needs an iroh endpoint and a peer that
//! answers, and every line below would otherwise be reachable only from a
//! two-machine run.

use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::test_support_remote::RecordingEmitter;

/// A slot holding one connection, named by generation.
fn holding(generation: u64) -> Mutex<Slot<u64>> {
    let mut slot = Slot::Empty;
    slot.reserve(generation).expect("a fresh slot is empty");
    assert!(slot.install(generation, generation));
    Mutex::new(slot)
}

/// A stand-in for `ConnectHandle::shutdown_timeout`, and whether it ran.
///
/// Which of the two matters more than it looks: on the unreachable path
/// modelpipe is still re-dialling behind the local port, so a `conclude`
/// that cleared the slot without this would leave a bound port and a
/// re-dial loop belonging to a connection nothing can reach any more.
#[derive(Default)]
struct Teardown(AtomicBool);

impl Teardown {
    /// The `shutdown` closure `conclude` is handed.
    async fn run(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    fn ran(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Whether a recorded run announced a disconnection.
fn announced(events: &RecordingEmitter) -> bool {
    events
        .events()
        .iter()
        .any(|e| matches!(e, AppEvent::RemoteDisconnected))
}

/// A peer given up on has its port dropped and its loss announced.
///
/// The other half of 5.4, and the half a person sees. The dwell deciding
/// the peer is gone changes nothing on its own: `remote status` reads
/// `live_connect`, so until the slot is cleared it still reports
/// "Connected", the loopback port is still bound in front of a machine that
/// is not there, and no GUI has been told anything.
#[tokio::test]
async fn a_peer_given_up_on_is_cleared_and_its_loss_announced() {
    let live = holding(7);
    let torn = Teardown::default();
    let events = RecordingEmitter::default();

    let cleared = conclude(
        &live,
        |live| *live == 7,
        Over::Unreachable,
        || torn.run(),
        &events,
    )
    .await;

    assert!(cleared);
    assert!(
        live.lock().await.full().is_none(),
        "the slot was not cleared"
    );
    assert!(
        torn.ran(),
        "the local port was left bound in front of a machine that is gone"
    );
    assert!(announced(&events), "{:?}", events.events());
}

/// A closed pipe ends the same way. modelpipe's verdict and this side's
/// differ in what they warn about and in nothing else.
#[tokio::test]
async fn a_closed_pipe_is_cleared_and_announced_the_same_way() {
    let live = holding(7);
    let torn = Teardown::default();
    let events = RecordingEmitter::default();

    assert!(
        conclude(
            &live,
            |live| *live == 7,
            Over::Closed,
            || torn.run(),
            &events
        )
        .await
    );
    assert!(live.lock().await.full().is_none());
    assert!(torn.ran());
    assert!(announced(&events));
}

/// A cancelled watcher touches nothing at all.
///
/// `disconnect` cancels the watcher, drains the handle and emits on its own
/// way out. A watcher that also emitted would announce one disconnection
/// twice, which a GUI counting them reads as two connections lost — and a
/// watcher that also shut down would drain a handle `disconnect` is holding.
#[tokio::test]
async fn a_cancelled_watcher_clears_nothing_and_announces_nothing() {
    let live = holding(7);
    let torn = Teardown::default();
    let events = RecordingEmitter::default();

    let cleared = conclude(
        &live,
        |live| *live == 7,
        Over::Cancelled,
        || torn.run(),
        &events,
    )
    .await;

    assert!(!cleared);
    assert_eq!(
        live.lock().await.full(),
        Some(&7),
        "a cancelled watcher took down the connection anyway"
    );
    assert!(!torn.ran());
    assert!(!announced(&events), "{:?}", events.events());
}

/// A watcher that outlived its own connection takes down the one that
/// replaced it — no.
///
/// What `generation` is for. Without it the sequence connect, disconnect,
/// connect leaves the first watcher able to clear the second connection and
/// announce a loss that never happened, and the person who just reconnected
/// watches it drop for no reason they can see.
#[tokio::test]
async fn a_watcher_whose_connection_was_replaced_leaves_the_replacement_alone() {
    let live = holding(8);
    let torn = Teardown::default();
    let events = RecordingEmitter::default();

    let cleared = conclude(
        &live,
        |live| *live == 7,
        Over::Unreachable,
        || torn.run(),
        &events,
    )
    .await;

    assert!(!cleared);
    assert_eq!(
        live.lock().await.full(),
        Some(&8),
        "the watcher cleared a connection that was not its own"
    );
    assert!(
        !torn.ran(),
        "and drained a handle belonging to somebody else"
    );
    assert!(!announced(&events), "{:?}", events.events());
}

/// The same watcher leaves a *dial* alone too.
///
/// A reservation is not something `take_if` can see, so the connect that
/// replaced this one is safe from the moment it reserves the slot rather
/// than from the moment it installs — which matters, because the gap
/// between those two is the length of a dial.
#[tokio::test]
async fn a_late_watcher_leaves_a_dial_that_replaced_it_alone() {
    let mut slot = Slot::<u64>::Empty;
    let cancel = slot.reserve(9).expect("a fresh slot is empty");
    let live = Mutex::new(slot);
    let torn = Teardown::default();
    let events = RecordingEmitter::default();

    let cleared = conclude(
        &live,
        |live| *live == 7,
        Over::Unreachable,
        || torn.run(),
        &events,
    )
    .await;

    assert!(!cleared);
    assert!(!torn.ran());
    assert!(!cancel.is_cancelled(), "the watcher cancelled a live dial");
    assert!(!announced(&events), "{:?}", events.events());
}
