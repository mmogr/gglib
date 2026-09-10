//! Tests for the record settings keep of the machine this one paired with.
//!
//! Everything `connect` decides *after* its dial has come up is here rather
//! than in `connect_tests.rs`, and that is the point of [`settle`] existing
//! at all: `modelpipe::connect` wants an iroh endpoint and a peer that
//! answers, so a test driven through `connect` never reaches the arm it is
//! about. It reaches the guards in front of the dial and stops. `settle`
//! takes the redemption as a closure, so both arms can be driven here with
//! no tunnel and no far machine.
//!
//! The tickets and the keys are in `test_support_remote.rs`, beside the
//! fixture, because three test modules name the same machines.

use super::*;
use crate::test_support::test_core;
use crate::test_support_remote::{KEY_A, KEY_B, TICKET_A, TICKET_A_MOVED, TICKET_B, paired_with};

/// A plausible six-digit code, never checked here: what the far machine
/// makes of it is the far machine's, and `redeem` is the seam.
const CODE: &str = "483920";

/// The redemption a codeless dial must not reach. A closure that cannot be
/// called is a stronger claim than one that records that it was not.
async fn never_redeems(_code: String) -> Result<String, GuiError> {
    unreachable!("a dial with no code has nothing to redeem")
}

fn ticket(s: &str) -> Ticket {
    s.parse().expect("a normative ticket vector parses")
}

/// A second pairing replaces the first whole, rather than half of it.
///
/// The inversion of the characterisation this test replaces, which pinned
/// that `remember(None, ticket_b)` left machine A's key sitting under
/// machine B's ticket. There is no longer a call that can do it: the two
/// halves are one `RemotePairing` and `remember` writes both or neither, so
/// the key that outlives the machine that issued it has no shape to live in.
/// A pairing as `remember` is handed one.
fn record(ticket: &str, api_key: &str) -> RemotePairing {
    RemotePairing {
        ticket: ticket.to_owned(),
        api_key: api_key.to_owned(),
        default_model: None,
    }
}

#[tokio::test]
async fn a_second_pairing_replaces_the_first_whole_rather_than_half_of_it() {
    let core = test_core().await;

    remember(&core, record(TICKET_A, KEY_A))
        .await
        .expect("machine A's pairing is stored");
    remember(&core, record(TICKET_B, KEY_B))
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
    let core = test_core().await;

    let err = store_redeemed(&core, "   ".to_owned(), TICKET_A.to_owned())
        .await
        .expect_err("a blank key is not a key, and settings refuse it");
    let GuiError::Internal(message) = err else {
        panic!("a store that failed is not the caller's to fix: {err:?}");
    };
    assert!(message.contains("already spent"), "{message}");
    assert!(message.contains("gglib remote enable"), "{message}");
}

/// A redeemed code leaves the key filed under the ticket that was dialled.
///
/// The pairing arm end to end, minus the wire: the code goes to `redeem`,
/// and what comes back is stored against *this* dial's machine rather than
/// whatever the record named before.
#[tokio::test]
async fn a_redeemed_code_is_stored_under_the_ticket_that_was_dialled() {
    let core = test_core().await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("machine A's pairing is stored");

    let paired = settle(
        &core,
        &ticket(TICKET_B),
        None,
        Some(CODE.to_owned()),
        async |code| {
            assert_eq!(code, CODE, "the code redeemed was not the code parsed");
            Ok(KEY_B.to_owned())
        },
    )
    .await
    .expect("the far machine handed a key back and settings took it");

    assert!(paired, "a redeemed code is a pairing");
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

/// The failure a spent code leaves behind reaches the caller of `connect`,
/// not just the caller of `store_redeemed`.
///
/// This is the arm the wording exists for, and the reason it is not
/// `remember`'s: by the time the write is attempted the code is gone at both
/// ends of the far machine, so "could not store the pairing" — which reads
/// as *try again* — is the one sentence a person must not be left with. The
/// test that calls `store_redeemed` directly says the function is right; it
/// cannot say the pairing arm is the caller that takes it, and swapping the
/// two here is a change nothing else notices.
#[tokio::test]
async fn a_redeemed_key_that_cannot_be_stored_says_the_code_is_already_spent() {
    let core = test_core().await;

    let err = settle(
        &core,
        &ticket(TICKET_B),
        None,
        Some(CODE.to_owned()),
        // A far side that answers with a blank key: the failure mode the
        // store really has, and the code is spent either way.
        async |_code| Ok("   ".to_owned()),
    )
    .await
    .expect_err("a blank key is not a key, and settings refuse it");

    let GuiError::Internal(message) = err else {
        panic!("a store that failed is not the caller's to fix: {err:?}");
    };
    assert!(message.contains("already spent"), "{message}");
    assert!(message.contains("gglib remote enable"), "{message}");
    assert!(
        core.settings()
            .get()
            .await
            .expect("settings load")
            .remote_pairing
            .is_none(),
        "a refused write left half a record behind"
    );
}

/// The same machine at a new address carries its key forward.
///
/// What makes the ticket the mutable half of the record. A machine hands out
/// a different ticket on every `enable` and at every address change, so a
/// codeless dial with the newer string is the same pairing, not a new one —
/// and the key, which only that machine could have issued, has to come with
/// it. Drop the write and the record keeps naming an address the machine has
/// left.
#[tokio::test]
async fn the_same_machine_at_a_new_address_carries_its_key_forward() {
    let core = test_core().await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("machine A's pairing is stored");
    let held = RemotePairing {
        ticket: TICKET_A.to_owned(),
        api_key: KEY_A.to_owned(),
        default_model: None,
    };
    let moved = ticket(TICKET_A_MOVED);
    assert_eq!(
        moved.fingerprint(),
        ticket(TICKET_A).fingerprint(),
        "the premise: these two strings are one machine"
    );

    let paired = settle(&core, &moved, Some(&held), None, never_redeems)
        .await
        .expect("a dial with no code still owes the record the new ticket");

    assert!(!paired, "no code was redeemed, so nothing was paired");
    let stored = core
        .settings()
        .get()
        .await
        .expect("settings load")
        .remote_pairing
        .expect("a pairing is stored");
    assert_eq!(
        stored.ticket,
        moved.to_string(),
        "the record did not follow the machine to its new address"
    );
    assert_ne!(
        stored.ticket, TICKET_A,
        "the record still names the address the machine left"
    );
    assert_eq!(
        stored.api_key, KEY_A,
        "the key that machine issued did not come forward with it"
    );
}

/// A dial to the machine already recorded writes nothing.
///
/// The other half of the same claim, and the reason the write is behind a
/// guard rather than unconditional: re-dialling the stored ticket is the
/// ordinary case, and it has nothing to teach settings. `held` is a
/// parameter here rather than something `settle` reads, which is what makes
/// the absence visible — a store left empty stays empty only if no write
/// happened at all.
#[tokio::test]
async fn a_dial_to_the_machine_already_recorded_writes_nothing() {
    let core = test_core().await;
    let held = RemotePairing {
        ticket: TICKET_A.to_owned(),
        api_key: KEY_A.to_owned(),
        default_model: None,
    };

    let paired = settle(&core, &ticket(TICKET_A), Some(&held), None, never_redeems)
        .await
        .expect("re-dialling the stored ticket is not a failure");

    assert!(!paired, "no code was redeemed, so nothing was paired");
    assert!(
        core.settings()
            .get()
            .await
            .expect("settings load")
            .remote_pairing
            .is_none(),
        "a dial to the machine already recorded wrote the record back"
    );
}
