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
//!
//! The teardown and the emitter are **one** fake writing to **one** log,
//! rather than two that each record only that they were called. `conclude`
//! claims an order — the port goes first — and two independent flags cannot
//! tell that order from its reverse.

use super::*;

/// A slot holding one connection, named by generation.
fn holding(generation: u64) -> Mutex<Slot<u64>> {
    let mut slot = Slot::Empty;
    slot.reserve(generation).expect("a fresh slot is empty");
    assert!(slot.install(generation, generation));
    Mutex::new(slot)
}

/// What `conclude` did, in the order it did it.
///
/// It stands in for `ConnectHandle::shutdown_timeout` and for the emitter at
/// once, because the claim is about the two together. That the port is
/// dropped matters on its own — on the unreachable path modelpipe is still
/// re-dialling behind it, so a `conclude` that cleared the slot without this
/// would leave a bound port and a re-dial loop belonging to a connection
/// nothing can reach any more. That it is dropped *first* is the other half:
/// a GUI told the connection is gone redraws immediately, and a port still
/// bound behind that redraw answers 502 for a machine the person has already
/// been told about.
#[derive(Default)]
struct Steps(std::sync::Mutex<Vec<&'static str>>);

/// The port dropped, in the log.
const SHUTDOWN: &str = "shutdown";
/// The loss announced, in the log.
const ANNOUNCED: &str = "announced";

impl Steps {
    /// The `shutdown` closure `conclude` is handed.
    async fn run(&self) {
        self.note(SHUTDOWN);
    }

    fn note(&self, step: &'static str) {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(step);
    }

    /// Everything that happened, oldest first.
    fn taken(&self) -> Vec<&'static str> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl AppEventEmitter for Steps {
    /// Only the disconnection is a step. `conclude` emits nothing else, and
    /// a log that recorded everything would pin the wrong thing.
    fn emit(&self, event: AppEvent) {
        if matches!(event, AppEvent::RemoteDisconnected) {
            self.note(ANNOUNCED);
        }
    }
}

/// A peer given up on has its port dropped and *then* its loss announced.
///
/// The other half of 5.4, and the half a person sees. The dwell deciding
/// the peer is gone changes nothing on its own: `remote status` reads
/// `live_connect`, so until the slot is cleared it still reports
/// "Connected", the loopback port is still bound in front of a machine that
/// is not there, and no GUI has been told anything.
///
/// The order is asserted, not just the two calls. It is the claim
/// `conclude`'s own doc makes, and the two are indistinguishable to a test
/// that only asks whether each happened.
#[tokio::test]
async fn a_peer_given_up_on_is_cleared_and_its_loss_announced() {
    let live = holding(7);
    let steps = Steps::default();

    let cleared = conclude(
        &live,
        |live| *live == 7,
        Over::Unreachable,
        || steps.run(),
        &steps,
    )
    .await;

    assert!(cleared);
    assert!(
        live.lock().await.full().is_none(),
        "the slot was not cleared"
    );
    assert_eq!(
        steps.taken(),
        [SHUTDOWN, ANNOUNCED],
        "the port is dropped before the loss is announced, or a GUI redraws in front of a port \
         that is still bound"
    );
}

/// A closed pipe ends the same way, in the same order. modelpipe's verdict
/// and this side's differ in what they warn about and in nothing else.
#[tokio::test]
async fn a_closed_pipe_is_cleared_and_announced_the_same_way() {
    let live = holding(7);
    let steps = Steps::default();

    assert!(
        conclude(
            &live,
            |live| *live == 7,
            Over::Closed,
            || steps.run(),
            &steps
        )
        .await
    );
    assert!(live.lock().await.full().is_none());
    assert_eq!(steps.taken(), [SHUTDOWN, ANNOUNCED]);
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
    let steps = Steps::default();

    let cleared = conclude(
        &live,
        |live| *live == 7,
        Over::Cancelled,
        || steps.run(),
        &steps,
    )
    .await;

    assert!(!cleared);
    assert_eq!(
        live.lock().await.full(),
        Some(&7),
        "a cancelled watcher took down the connection anyway"
    );
    assert!(steps.taken().is_empty(), "{:?}", steps.taken());
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
    let steps = Steps::default();

    let cleared = conclude(
        &live,
        |live| *live == 7,
        Over::Unreachable,
        || steps.run(),
        &steps,
    )
    .await;

    assert!(!cleared);
    assert_eq!(
        live.lock().await.full(),
        Some(&8),
        "the watcher cleared a connection that was not its own"
    );
    assert!(
        steps.taken().is_empty(),
        "it drained a handle belonging to somebody else, or announced their loss: {:?}",
        steps.taken()
    );
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
    let steps = Steps::default();

    let cleared = conclude(
        &live,
        |live| *live == 7,
        Over::Unreachable,
        || steps.run(),
        &steps,
    )
    .await;

    assert!(!cleared);
    assert!(!cancel.is_cancelled(), "the watcher cancelled a live dial");
    assert!(steps.taken().is_empty(), "{:?}", steps.taken());
}
