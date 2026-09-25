//! Tests for the record settings keep of the machine this one paired with.
//!
//! Everything `join` decides *after* its dial has come up is here rather
//! than in `connect_tests.rs`, and that is the point of [`settle`] existing
//! at all: `modelpipe::connect` wants an iroh endpoint and a peer that
//! answers, so a test driven through `join` never reaches the arm it is
//! about. It reaches the guards in front of the dial and stops. `settle`
//! takes the redemption as a closure, so both arms can be driven here with
//! no tunnel and no far machine.
//!
//! The tickets and the keys are in `test_support_remote.rs`, beside the
//! fixture, because three test modules name the same machines.

use super::*;
use crate::test_support::test_core;
use crate::test_support_remote::{
    KEY_A, KEY_B, TICKET_A, TICKET_A_MOVED, TICKET_B, paired_with, remember_a_model, ticket,
};

/// A plausible six-digit code, never checked here: what the far machine
/// makes of it is the far machine's, and `redeem` is the seam.
const CODE: &str = "483920";

/// The key machine A hands over when it is paired with a second time.
const KEY_A_AGAIN: &str = "sk-zzq-the-key-machine-a-handed-over-again";

/// A model name, as a `--remote` turn remembers one on the stored pairing.
const MODEL: &str = "qwen3-coder";

/// The stored pairing, which each test here expects to exist.
async fn stored(core: &AppCore) -> RemotePairing {
    core.settings()
        .get()
        .await
        .expect("settings load")
        .remote_pairing
        .expect("a pairing is stored")
}

/// A second pairing replaces the first whole, rather than half of it, and
/// starts with nothing remembered.
///
/// The ticket and the key are one record, written together, so machine A's
/// key has no shape to outlive machine A in. Nor does its model: that is a
/// name in machine A's catalogue, not in machine B's.
#[tokio::test]
async fn a_second_pairing_replaces_the_first_whole_rather_than_half_of_it() {
    let core = test_core().await;

    store_redeemed(&core, KEY_A.to_owned(), &ticket(TICKET_A), 8180)
        .await
        .expect("machine A's pairing is stored");
    remember_a_model(&core, MODEL).await;
    store_redeemed(&core, KEY_B.to_owned(), &ticket(TICKET_B), 8180)
        .await
        .expect("machine B's pairing replaces it");

    let stored = stored(&core).await;
    assert_eq!(stored.ticket, TICKET_B);
    assert_eq!(stored.api_key, KEY_B);
    assert_eq!(
        stored.default_model, None,
        "machine A's model was carried to machine B"
    );
}

/// Pair with machine A again, dialling `dialled`, while a `--remote` turn
/// remembers a model on the record machine A's first pairing left, and
/// return what is stored afterwards.
///
/// `redeem` is where the dial spends its time, so a turn run in a terminal
/// meanwhile lands its write there.
async fn pair_with_machine_a_again_during_a_turn(core: &AppCore, dialled: &str) -> RemotePairing {
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("machine A's pairing is stored");
    let held = stored(core).await;

    let paired = settle(
        core,
        &ticket(dialled),
        Some(&held),
        Some(CODE.to_owned()),
        8181,
        async |_code| {
            remember_a_model(core, MODEL).await;
            Ok(KEY_A_AGAIN.to_owned())
        },
    )
    .await
    .expect("the far machine handed a key back and settings took it");

    assert!(paired, "a redeemed code is a pairing");
    stored(core).await
}

/// A model remembered while a pairing dial was under way survives the
/// pairing it raced.
///
/// The pairing is with the machine the record already names, so the model
/// is still a name in that machine's catalogue, and the new key is filed
/// beside it.
#[tokio::test]
async fn a_model_remembered_during_the_dial_survives_the_pairing() {
    let core = test_core().await;

    let stored = pair_with_machine_a_again_during_a_turn(&core, TICKET_A).await;

    assert_eq!(
        stored.default_model.as_deref(),
        Some(MODEL),
        "the pairing dropped a model remembered on machine A's record during the dial"
    );
    assert_eq!(
        stored.api_key, KEY_A_AGAIN,
        "the redeemed key was not stored"
    );
    assert_eq!(
        stored.port,
        Some(8181),
        "the port just bound was not stored"
    );
}

/// The model survives a pairing with the same machine at a new address.
///
/// The ticket differs and the fingerprint does not, and the fingerprint is
/// what says which machine will answer, so the model is still a name in its
/// catalogue.
#[tokio::test]
async fn a_model_survives_a_pairing_with_the_same_machine_at_a_new_address() {
    let core = test_core().await;

    let stored = pair_with_machine_a_again_during_a_turn(&core, TICKET_A_MOVED).await;

    assert_eq!(
        stored.ticket,
        ticket(TICKET_A_MOVED).to_string(),
        "the pairing was not filed under the ticket just dialled"
    );
    assert_eq!(
        stored.default_model.as_deref(),
        Some(MODEL),
        "machine A at a new address was taken for another machine"
    );
    assert_eq!(
        stored.api_key, KEY_A_AGAIN,
        "the redeemed key was not stored"
    );
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

    let err = store_redeemed(&core, "   ".to_owned(), &ticket(TICKET_A), 8180)
        .await
        .expect_err("a blank key is not a key, and settings refuse it");
    let GuiError::Internal(message) = err else {
        panic!("a store that failed is not the caller's to fix: {err:?}");
    };
    assert!(message.contains("already spent"), "{message}");
    assert!(message.contains("gglib remote invite"), "{message}");
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
        8180,
        async |code| {
            assert_eq!(code, CODE, "the code redeemed was not the code parsed");
            Ok(KEY_B.to_owned())
        },
    )
    .await
    .expect("the far machine handed a key back and settings took it");

    assert!(paired, "a redeemed code is a pairing");
    let stored = stored(&core).await;
    assert_eq!(stored.ticket, TICKET_B);
    assert_eq!(stored.api_key, KEY_B);
}

/// The failure a spent code leaves behind reaches the caller of `join`,
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
        8180,
        async |_code| Ok("   ".to_owned()),
    )
    .await
    .expect_err("a blank key is not a key, and settings refuse it");

    let GuiError::Internal(message) = err else {
        panic!("a store that failed is not the caller's to fix: {err:?}");
    };
    assert!(message.contains("already spent"), "{message}");
    assert!(message.contains("gglib remote invite"), "{message}");
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
