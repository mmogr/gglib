//! Tests for the connect side — this machine as the laptop.
//!
//! Everything here stops short of `modelpipe::connect`, which wants an iroh
//! endpoint and a peer that answers: what `connect` refuses before it dials,
//! and what `remember` leaves behind in settings once it has. The one test
//! that does dial is `#[ignore]`d and says why on itself.
//!
//! The tickets are modelpipe's own normative vectors from
//! `docs/ticket-format-v0.md`, which ship in its published tarball and are
//! asserted identical by three implementations on every one of its CI runs.
//! `ticket_vectors.py` has no `--update` flag, deliberately, so these
//! strings cannot drift under us.

use std::time::Duration;

use gglib_core::SettingsUpdate;

use super::*;
use crate::test_support_remote::test_remote_ops;

/// Vector 1: the minimal v0 ticket, no transport addresses.
const TICKET_A: &str = "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaaauhlp2na";

/// The first six bytes of vector 1's endpoint id, which is what a
/// fingerprint shows.
const FINGERPRINT_A: &str = "d75a980182b1";

/// A *second* machine: the same minimal shape as vector 1 over the public
/// key from RFC 8032 §7.1 TEST 2, so the two tickets name genuinely
/// different endpoints rather than one endpoint at two addresses. Every
/// published vector shares TEST 1's key, so no pair of them could say this.
const TICKET_B: &str = "pipeaa6uaf6d5bbyswusw4fkoti3p26jzgbmz4xmjfumydgvl4jk6rtayaaa2e4g6hq";

/// Vector 1's key with vector 3's address set: one IPv6 address in the
/// documentation prefix (RFC 3849), which routes nowhere anywhere. The only
/// ticket here that is ever dialled.
const TICKET_UNREACHABLE: &str = "pipeadlvvgabqkyqvn6vjp7nhslea45a5yls6pnkmizfv4bbu2hxa5iruaicaajcaainxaaaaaaaaaaaaaaaaaaach4qaabstehw";

const KEY_A: &str = "sk-zzq-the-key-machine-a-handed-over";

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
        .update(SettingsUpdate {
            remote_api_key: Some(Some(KEY_A.to_owned())),
            ..SettingsUpdate::default()
        })
        .await
        .expect("a key is stored");

    let err = ops.kill_remote().await.expect_err("nothing to stop");
    let GuiError::Conflict(message) = err else {
        panic!("not being connected is a conflict: {err:?}");
    };
    assert!(message.contains("not connected"), "{message}");
}

/// **Characterisation, and the defect is the point.** The two settings
/// fields move independently, so this machine can end up holding machine
/// A's key filed under machine B's ticket.
///
/// `remember` is the only writer, and the bare-ticket arm of `connect`
/// calls it with `None` for the key — which does not clear the old key, it
/// leaves it exactly where it was. Written as it behaves rather than as it
/// should, so the fix cannot land silently: the PR that makes a stored key
/// name its machine inverts this assertion, and the `#[ignore]`d test below
/// is the claim it makes true.
#[tokio::test]
async fn a_bare_ticket_connect_keeps_the_previous_machines_key() {
    let (core, ops, _) = test_remote_ops().await;

    ops.remember(Some(KEY_A.to_owned()), TICKET_A.to_owned())
        .await
        .expect("machine A's pairing is stored");
    // What the `code.is_none()` arm of `connect` does on a dial to a
    // machine this one has never paired with.
    ops.remember(None, TICKET_B.to_owned())
        .await
        .expect("machine B's ticket is stored");

    let settings = core.settings().get().await.expect("settings load");
    assert_eq!(settings.remote_last_ticket.as_deref(), Some(TICKET_B));
    assert_eq!(
        settings.remote_api_key.as_deref(),
        Some(KEY_A),
        "today the key outlives the machine that issued it"
    );
}

/// The claim the PR that makes a stored key name its machine has to satisfy:
/// whatever settings remember of a pairing describes **one** machine.
///
/// It fails today, which is why it is here and `#[ignore]`d rather than
/// absent. A key is issued by the machine whose ticket was redeemed, so a
/// key that survives a change of ticket is a key for nobody: presented to
/// machine B it is a 401 that gglib renders as "expired, used already, or
/// burned by wrong attempts", none of which is true. Binding the two into
/// one record keyed by ticket fingerprint makes the disagreement above
/// unrepresentable rather than merely wrong.
#[tokio::test]
#[ignore = "pending: the stored pairing is still two independent settings rows"]
async fn the_stored_key_and_the_stored_ticket_describe_one_machine() {
    let (core, ops, _) = test_remote_ops().await;

    ops.remember(Some(KEY_A.to_owned()), TICKET_A.to_owned())
        .await
        .expect("machine A's pairing is stored");
    ops.remember(None, TICKET_B.to_owned())
        .await
        .expect("machine B's ticket is stored");

    let settings = core.settings().get().await.expect("settings load");
    let fingerprint = settings
        .remote_last_ticket
        .as_deref()
        .and_then(|t| t.parse::<modelpipe::Ticket>().ok())
        .map(|t| t.fingerprint());
    assert_ne!(
        fingerprint.as_deref(),
        Some(FINGERPRINT_A),
        "the stored ticket is machine B's"
    );
    assert!(
        settings.remote_api_key.is_none(),
        "machine B never handed this machine a key, so there is none to hold"
    );
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
        .update(SettingsUpdate {
            remote_api_key: Some(Some(KEY_A.to_owned())),
            ..SettingsUpdate::default()
        })
        .await
        .expect("a stored key admits a bare ticket");

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
