//! Tests for the gate between binding a port and using it.
//!
//! On a paused clock, so the thirty seconds cost nothing and the budget
//! under test is the shipped one rather than a shortened copy: tokio
//! advances time itself once every task is waiting, which is the shape of
//! this loop — one deadline, one cancellation and one status to wait for.
//!
//! The statuses arrive through a channel rather than from a real
//! `ConnectHandle`, which would want an iroh endpoint and a peer to answer,
//! and what a dial pays for arrives as a future that records whether it was
//! polled. That second one is the point of the file: the claim is not that
//! the wait works, it is that *nothing is spent* when it fails.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::*;

/// The statuses a test hands to [`wait`], and the handle it sends them on.
type Source = Mutex<UnboundedReceiver<PipeStatus>>;

fn source() -> (UnboundedSender<PipeStatus>, Source) {
    let (tx, rx) = unbounded_channel();
    (tx, Mutex::new(rx))
}

/// The next status, or nothing ever again.
///
/// A closed channel is a pipe that has stopped changing, which is what
/// never being reached *is* — so this waits for ever and leaves the deadline
/// as the only thing that can still fire.
async fn next_from(source: &Source) -> PipeStatus {
    match source.lock().await.recv().await {
        Some(status) => status,
        None => std::future::pending().await,
    }
}

/// **Every call to [`wait`] here is wrapped in a timeout**, for
/// `connect_watch_tests`' reason: two of its arms never resolve on their
/// own, so a regression that stops the deadline arming does not fail these
/// tests, it hangs them — and a hung test takes the job's whole budget
/// instead of naming the line that broke. The paused clock makes the
/// wrapper free.
const RUNAWAY: Duration = FIRST_CONTACT.saturating_mul(3);

/// A machine that never answers is given up on rather than waited on for
/// ever, and the sentence says what to check.
#[tokio::test(start_paused = true)]
async fn a_machine_that_never_answers_is_given_up_on_at_the_budget() {
    let (tx, rx) = source();
    tx.send(PipeStatus::Idle).expect("the gate is listening");
    drop(tx);

    let waited = tokio::time::timeout(
        RUNAWAY,
        wait(PipeStatus::Idle, || next_from(&rx), &CancellationToken::new()),
    )
    .await
    .expect("the deadline never fired, so the dial is still waiting");

    assert_eq!(waited.err(), Some(NoContact::Never));
    let GuiError::Unavailable(message) = refusal(&NoContact::Never) else {
        panic!("a machine that is off is not the caller's to fix");
    };
    assert!(message.contains("did not answer within 30 seconds"), "{message}");
    assert!(message.contains("gglib remote enable"), "{message}");
}

/// A relayed path is contact. Reading only `Direct` would refuse every
/// pairing behind a carrier NAT, which is the case the relay exists for.
#[tokio::test(start_paused = true)]
async fn a_relayed_path_is_first_contact_just_as_a_direct_one_is() {
    for reached in [PipeStatus::Direct, PipeStatus::Relayed] {
        let (tx, rx) = source();
        tx.send(reached).expect("the gate is listening");

        let waited = tokio::time::timeout(
            RUNAWAY,
            wait(PipeStatus::Idle, || next_from(&rx), &CancellationToken::new()),
        )
        .await
        .expect("a path formed, so the gate had its answer");

        assert!(waited.is_ok(), "{reached:?} is a path to the far machine");
    }
}

/// The starting status is read before anything is waited for. A pipe that
/// found its peer between the bind and the first `status_changed` has
/// nothing left to report, so a gate that only listened would spend the
/// whole budget on a connection that was already up.
#[tokio::test(start_paused = true)]
async fn a_pipe_that_connected_before_the_gate_looked_is_not_waited_for() {
    let (tx, rx) = source();
    // Nothing is ever sent: the only status this test offers is the initial
    // one, and a gate that ignored it would sit on the deadline.
    drop(tx);

    let waited = tokio::time::timeout(
        RUNAWAY,
        wait(
            PipeStatus::Direct,
            || next_from(&rx),
            &CancellationToken::new(),
        ),
    )
    .await
    .expect("the pipe was connected before the gate started");

    assert!(waited.is_ok(), "a connected pipe is contact");
}

/// A pipe that ends before it connects is not waited out to the budget —
/// there is nothing left to wait for, and the sentence says the far machine
/// was never involved.
#[tokio::test(start_paused = true)]
async fn a_pipe_that_closes_before_it_connects_ends_the_wait_at_once() {
    let (tx, rx) = source();
    tx.send(PipeStatus::Closed).expect("the gate is listening");

    let waited = tokio::time::timeout(
        RUNAWAY,
        wait(PipeStatus::Idle, || next_from(&rx), &CancellationToken::new()),
    )
    .await
    .expect("a closed pipe is an answer");

    assert_eq!(waited.err(), Some(NoContact::Closed));
    let GuiError::Unavailable(message) = refusal(&NoContact::Closed) else {
        panic!("a listener that died is not the caller's to fix");
    };
    assert!(message.contains("closed before the remote"), "{message}");
}

/// `gglib remote disconnect` ends the wait, and is reported as the conflict
/// it is rather than as a machine that failed to answer. Thirty seconds is
/// long enough that a person who changed their mind should not be held for
/// it.
#[tokio::test(start_paused = true)]
async fn a_disconnect_ends_the_wait_rather_than_being_queued_behind_it() {
    let (tx, rx) = source();
    tx.send(PipeStatus::Idle).expect("the gate is listening");
    let cancel = CancellationToken::new();
    let cancelling = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_CONTACT / 3).await;
        cancelling.cancel();
        // Held open, so the gate waits on a status that never comes rather
        // than on a closed channel.
        tokio::time::sleep(FIRST_CONTACT * 10).await;
        drop(tx);
    });

    let waited = tokio::time::timeout(RUNAWAY, wait(PipeStatus::Idle, || next_from(&rx), &cancel))
        .await
        .expect("the cancellation was not heard, so the dial ran on for nobody");

    assert_eq!(waited.err(), Some(NoContact::Cancelled));
    assert!(
        matches!(refusal(&NoContact::Cancelled), GuiError::Conflict(_)),
        "giving up on a dial is not a failure of the dial"
    );
}

/// The claim the gate exists for: what a dial pays is not paid when the far
/// machine never answered.
///
/// The cost is the far machine's one-time code, and there is no way to get
/// it back from this side — so "the request went out and failed" is not the
/// same outcome as "the request was never made", which is what this asserts.
#[tokio::test(start_paused = true)]
async fn nothing_is_spent_when_the_machine_was_never_reached() {
    for no in [NoContact::Never, NoContact::Closed, NoContact::Cancelled] {
        let spent = Arc::new(AtomicBool::new(false));
        let watching = Arc::clone(&spent);

        let out: Result<(), GuiError> = once_reached(async { Err(no) }, |_reached| async move {
            watching.store(true, Ordering::SeqCst);
            Ok(())
        })
        .await;

        assert!(out.is_err(), "a dial that reached nobody cannot succeed");
        assert!(
            !spent.load(Ordering::SeqCst),
            "the one-time code was redeemed into a pipe that reached nobody"
        );
    }
}

/// And it *is* paid when the machine answered — the other half, without
/// which the assertion above passes on a gate that refuses everything.
#[tokio::test(start_paused = true)]
async fn what_the_dial_pays_for_is_paid_once_the_machine_answers() {
    let spent = Arc::new(AtomicBool::new(false));
    let watching = Arc::clone(&spent);

    let out: Result<&str, GuiError> =
        once_reached(async { Ok(Reached(())) }, |_reached| async move {
            watching.store(true, Ordering::SeqCst);
            Ok("the far machine's key")
        })
        .await;

    assert_eq!(out.expect("contact was made"), "the far machine's key");
    assert!(spent.load(Ordering::SeqCst), "the pairing was never redeemed");
}
