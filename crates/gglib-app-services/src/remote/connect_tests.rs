//! Tests for the connect side — this machine as the laptop.
//!
//! Everything here stops short of `modelpipe::connect`, which wants an iroh
//! endpoint and a peer that answers: what `connect` refuses before it dials,
//! and what `remember` leaves behind in settings once it has. The one test
//! that does dial is `#[ignore]`d and says why on itself.
//!
//! The tickets and the keys are in `test_support_remote.rs`, beside the
//! fixture, because `lifecycle_tests.rs` names the same machines.

use std::time::Duration;

use super::*;
use crate::test_support_remote::{
    FINGERPRINT_A, KEY_A, KEY_B, TICKET_A, TICKET_B, TICKET_UNREACHABLE, paired_with,
    test_remote_ops,
};

/// What `gglib remote status` allows the daemon before it gives up
/// (`gglib-cli/src/daemon_client/remote.rs`). A status that takes longer
/// than this is a status nobody sees.
const CLI_STATUS_TIMEOUT: Duration = Duration::from_secs(5);

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

/// A second pairing replaces the first whole, rather than half of it.
///
/// The inversion of the characterisation this test replaces, which pinned
/// that `remember(None, ticket_b)` left machine A's key sitting under
/// machine B's ticket. There is no longer a call that can do it: the two
/// halves are one `RemotePairing` and `remember` writes both or neither, so
/// the key that outlives the machine that issued it has no shape to live in.
#[tokio::test]
async fn a_second_pairing_replaces_the_first_whole_rather_than_half_of_it() {
    let (core, _ops, _) = test_remote_ops().await;

    remember(&core, KEY_A.to_owned(), TICKET_A.to_owned())
        .await
        .expect("machine A's pairing is stored");
    remember(&core, KEY_B.to_owned(), TICKET_B.to_owned())
        .await
        .expect("machine B's pairing replaces it");

    let stored = core
        .settings()
        .get()
        .await
        .expect("settings load")
        .remote_pairing
        .expect("a pairing is stored");
    assert_eq!(stored.ticket, TICKET_B);
    assert_eq!(stored.api_key, KEY_B);
}

/// A pairing that cannot be stored says the code has already been spent.
///
/// Finding A2, as far as this layer can carry it. `redeem` burns the code at
/// both ends of the far machine before the write is attempted, so a failure
/// here is not a retry — it is a trip to the other machine — and the
/// difference is entirely in what the message says. The write is made to
/// fail the way it really can: the far side answered with a blank key, which
/// `validate_settings` refuses, and the code is gone either way.
///
/// What it does **not** do is get the key back. Nothing here can: the key
/// exists only in the response just read, and the store that would have kept
/// it is what failed.
#[tokio::test]
async fn a_pairing_that_cannot_be_stored_says_the_code_is_already_spent() {
    let (core, _ops, _) = test_remote_ops().await;

    let err = store_redeemed(&core, "   ".to_owned(), TICKET_A.to_owned())
        .await
        .expect_err("a blank key is not a key, and settings refuse it");
    let GuiError::Internal(message) = err else {
        panic!("a store that failed is not the caller's to fix: {err:?}");
    };
    assert!(message.contains("already spent"), "{message}");
    assert!(message.contains("gglib remote enable"), "{message}");
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

/// `status` answers while a dial is in flight — finding A1, and it fails
/// today.
///
/// `connect` holds `live_connect` from its first line to its last, across
/// `modelpipe::connect` and a redeem that may take twenty seconds, while
/// `status` locks the same mutex to read the connect snapshot.
/// `tokio::sync::Mutex` is FIFO-fair, so a dial that is waiting out an
/// unreachable peer makes `gglib remote status` wait with it — past the
/// five seconds the CLI allows — and makes `gglib remote disconnect` unable
/// to cancel the very connect it exists to cancel. The fix is to check and
/// reserve under the lock, release across the dial, and re-acquire to
/// install; `connect_generation` already exists to make that safe.
///
/// `#[ignore]`d because it binds a real iroh endpoint, the way
/// `pidfile::sweep`'s real-directory test is ignored for touching the real
/// `pids_dir()`. It dials a ticket whose only address is in the IPv6
/// documentation prefix, with discovery off so that address is the only
/// path there is and nothing is resolved over the network. Run it with
/// `cargo test -p gglib-app-services -- --ignored`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "binds a real iroh endpoint; pending: connect holds its mutex across the dial"]
async fn status_answers_while_a_dial_is_in_flight() {
    let (core, ops, _) = test_remote_ops().await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("a pairing naming that machine admits a bare ticket for it");

    let dialling = Arc::clone(&ops);
    let dial = tokio::spawn(async move {
        dialling
            .connect(ConnectRequest {
                pairing: Some(TICKET_UNREACHABLE.to_owned()),
                discovery: false,
                ..ConnectRequest::default()
            })
            .await
    });
    // Long enough for the spawned task to have taken the lock, short enough
    // that the assertion below is still about the dial and not about it.
    tokio::time::sleep(Duration::from_millis(250)).await;

    let answered = tokio::time::timeout(CLI_STATUS_TIMEOUT, ops.status()).await;
    // A dial that gave up on its own released the lock, and then a fast
    // `status` proves nothing. Checked before the verdict so this cannot go
    // green on a machine where the address fails immediately for want of
    // any IPv6 route at all.
    let still_dialling = !dial.is_finished();
    dial.abort();
    assert!(
        still_dialling,
        "the dial ended by itself, so this run said nothing about the lock"
    );
    assert!(
        answered.is_ok(),
        "status waited on the dial's mutex past the timeout the CLI gives it"
    );
}
