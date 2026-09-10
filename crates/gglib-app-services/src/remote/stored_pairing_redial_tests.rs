//! Tests for what a dial with no code owes the record.
//!
//! [`settle`]'s codeless arm, split from `stored_pairing_tests.rs` when the
//! two arms together crossed the 300-line budget
//! `scripts/check_rust_complexity.sh` allows. The seam is the one the
//! function already has: a dial that carries a code is pairing, and
//! everything it decides is about the key that comes back; a dial without
//! one is a machine this laptop already knows, and everything it decides is
//! about keeping the record true — the address that machine now answers at,
//! and the port it answers on here. Two subjects, two files.

use super::*;
use crate::test_support::test_core;
use crate::test_support_remote::{
    KEY_A, TICKET_A, TICKET_A_MOVED, never_redeems, paired_with, ticket,
};

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
        port: None,
    };
    let moved = ticket(TICKET_A_MOVED);
    assert_eq!(
        moved.fingerprint(),
        ticket(TICKET_A).fingerprint(),
        "the premise: these two strings are one machine"
    );

    let paired = settle(&core, &moved, Some(&held), None, 8180, never_redeems)
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

/// A dial to the machine already recorded, on the port already recorded,
/// writes nothing.
///
/// The other half of the same claim, and the reason the write is behind a
/// guard rather than unconditional: re-dialling the stored ticket on the
/// stored port is the ordinary case, and it has nothing to teach settings.
/// `held` is a parameter here rather than something `settle` reads, which
/// is what makes the absence visible — a store left empty stays empty only
/// if no write happened at all.
#[tokio::test]
async fn a_dial_to_the_machine_already_recorded_writes_nothing() {
    let core = test_core().await;
    let held = RemotePairing {
        ticket: TICKET_A.to_owned(),
        api_key: KEY_A.to_owned(),
        default_model: None,
        port: Some(8180),
    };

    let paired = settle(
        &core,
        &ticket(TICKET_A),
        Some(&held),
        None,
        8180,
        never_redeems,
    )
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

/// A record that names no port learns the one this dial bound.
///
/// The record a 0.17 laptop holds predates the port field, and the record
/// of a dial that had to move off its port names the old one. Either way
/// the port just bound is the port a client should find next time, so the
/// redial is the one write that teaches it — same ticket, same key.
#[tokio::test]
async fn a_record_without_a_port_learns_the_port_this_dial_bound() {
    let core = test_core().await;
    let held = RemotePairing {
        ticket: TICKET_A.to_owned(),
        api_key: KEY_A.to_owned(),
        default_model: None,
        port: None,
    };

    let paired = settle(
        &core,
        &ticket(TICKET_A),
        Some(&held),
        None,
        8181,
        never_redeems,
    )
    .await
    .expect("re-dialling the stored ticket is not a failure");

    assert!(!paired, "no code was redeemed, so nothing was paired");
    let stored = core
        .settings()
        .get()
        .await
        .expect("settings load")
        .remote_pairing
        .expect("the port this dial bound was not written down");
    assert_eq!(
        stored.port,
        Some(8181),
        "the record names a port it never bound"
    );
    assert_eq!(
        stored.ticket, TICKET_A,
        "learning a port must not touch the ticket"
    );
    assert_eq!(
        stored.api_key, KEY_A,
        "learning a port must not touch the key"
    );
}
