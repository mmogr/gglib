//! Tests for the connect side — this machine as the laptop.
//!
//! Everything here stops short of `modelpipe::connect`, which wants an iroh
//! endpoint and a peer that answers: what `connect` refuses before it dials.
//! What it owes settings once the dial has come up is on the far side of
//! that call, so it is driven through `settle` in `stored_pairing_tests.rs`
//! instead. The one test that does dial is `#[ignore]`d and lives in
//! `connect_race_tests.rs`, beside the claim it makes.
//!
//! The tickets and the keys are in `test_support_remote.rs`, beside the
//! fixture, because `lifecycle_tests.rs` names the same machines. What
//! happens when two of these arrive at once is in `connect_race_tests.rs`.

use super::*;
use crate::test_support_remote::{
    FINGERPRINT_A, KEY_A, TICKET_A, TICKET_B, paired_with, test_remote_ops,
};

/// A machine that has never paired is told what to paste, not given a
/// generic failure — there is nothing in settings to dial and nothing to
/// authenticate with.
#[tokio::test]
async fn a_first_connect_with_no_argument_asks_for_the_pairing_string() {
    let (_, ops, events) = test_remote_ops().await;

    let err = ops
        .connect(ConnectRequest::default())
        .await
        .expect_err("nothing is stored, so there is nothing to dial");
    let GuiError::ValidationFailed(message) = err else {
        panic!("a first connect is the caller's to fix, not the daemon's: {err:?}");
    };
    assert!(
        message.contains("has not connected to a remote before"),
        "{message}"
    );
    assert!(events.events().is_empty(), "nothing happened to announce");
}

/// A bare ticket needs a key from an earlier pairing. Refusing here rather
/// than at the far edge is what keeps a dial off the wire that could only
/// ever end in a 401.
#[tokio::test]
async fn a_bare_ticket_with_no_stored_key_is_refused_before_anything_is_dialled() {
    let (_, ops, _) = test_remote_ops().await;

    let err = ops
        .connect(ConnectRequest {
            pairing: Some(TICKET_A.to_owned()),
            ..ConnectRequest::default()
        })
        .await
        .expect_err("no key is stored for that machine");
    let GuiError::ValidationFailed(message) = err else {
        panic!("holding no key is the caller's to fix: {err:?}");
    };
    assert!(message.contains("holds no key"), "{message}");
}

/// A pairing string that is not one is named as such, and the guard order
/// puts the parse before the key check: someone who typed the ticket wrong
/// hears about the ticket, not about a key they were never asked for.
#[tokio::test]
async fn a_pairing_string_that_does_not_parse_is_reported_as_the_typo_it_is() {
    let (_, ops, _) = test_remote_ops().await;

    let err = ops
        .connect(ConnectRequest {
            pairing: Some("not-a-ticket-483920".to_owned()),
            ..ConnectRequest::default()
        })
        .await
        .expect_err("that is not a ticket");
    let GuiError::ValidationFailed(message) = err else {
        panic!("a typo is the caller's to fix: {err:?}");
    };
    assert!(message.contains("not a ticket"), "{message}");
}

/// Disconnecting nothing is a conflict, not a silent success, and it does
/// not announce a disconnection that did not happen — a GUI that redrew on
/// the event would show a pipe going down that was never up.
#[tokio::test]
async fn disconnecting_when_nothing_is_connected_is_a_conflict_and_announces_nothing() {
    let (_, ops, events) = test_remote_ops().await;

    assert!(matches!(ops.disconnect().await, Err(GuiError::Conflict(_))));
    assert!(events.events().is_empty(), "{:?}", events.events());
}

/// `kill_remote` is a one-way door on *another* machine, so it refuses
/// before it looks at anything else when this machine is not connected to
/// one. The stored key is not consulted, which is the order that matters:
/// there is no base URL to send it to.
#[tokio::test]
async fn killing_a_remote_this_machine_is_not_connected_to_is_a_conflict() {
    let (core, ops, _) = test_remote_ops().await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("a pairing is stored");

    let err = ops.kill_remote().await.expect_err("nothing to stop");
    let GuiError::Conflict(message) = err else {
        panic!("not being connected is a conflict: {err:?}");
    };
    assert!(message.contains("not connected"), "{message}");
}

/// Whatever settings remember of a pairing describes **one** machine.
///
/// The claim PR 1 left `#[ignore]`d, satisfied here — though not the way
/// that test's body guessed. It assumed a bare-ticket dial to a second
/// machine would go ahead and record the new ticket with no key; the fix is
/// that the dial does not go ahead at all. A key is issued by the machine
/// whose code was redeemed, so a dial that can only end in a 401 is refused
/// before it reaches the wire, and the pairing this machine does hold is
/// left exactly as it was — machine A's ticket with machine A's key.
///
/// Refusing rather than dialling is the difference between the two
/// readings, and it is the one 5.5 asks for: the old guard admitted the
/// dial on "a key exists" and left `remote status` reporting a fully paired
/// connection to a machine that had never seen this one.
#[tokio::test]
async fn the_stored_key_and_the_stored_ticket_describe_one_machine() {
    let (core, ops, events) = test_remote_ops().await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("machine A's pairing is stored");

    let err = ops
        .connect(ConnectRequest {
            pairing: Some(TICKET_B.to_owned()),
            ..ConnectRequest::default()
        })
        .await
        .expect_err("machine B never handed this machine a key");
    let GuiError::ValidationFailed(message) = err else {
        panic!("holding somebody else's key is the caller's to fix: {err:?}");
    };
    assert!(message.contains("holds no key"), "{message}");

    let stored = core
        .settings()
        .get()
        .await
        .expect("settings load")
        .remote_pairing
        .expect("machine A's pairing is untouched");
    assert_eq!(
        stored.ticket, TICKET_A,
        "a refused dial rewrote the stored ticket"
    );
    assert_eq!(stored.api_key, KEY_A);
    assert!(events.events().is_empty(), "nothing happened to announce");

    // And the status surface can say *which* machine the key it reports is
    // for, which is the question a stale key left unanswerable.
    let status = ops.status().await;
    assert_eq!(
        status.stored_ticket_fingerprint.as_deref(),
        Some(FINGERPRINT_A)
    );
    assert!(status.has_remote_key);
}
