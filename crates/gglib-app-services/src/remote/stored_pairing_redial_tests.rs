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

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::{RepositoryError, SettingsRepository};
use gglib_db::{CoreFactory, setup_test_database};

use super::*;
use crate::test_support::test_core;
use crate::test_support_remote::{
    KEY_A, KEY_B, TICKET_A, TICKET_A_MOVED, TICKET_B, never_redeems, paired_with, remember_a_model,
    ticket,
};

/// A settings store that refuses every write, so that a call which succeeds
/// against it is a call that wrote nothing.
struct Unwritable(Arc<dyn SettingsRepository>);

#[async_trait]
impl SettingsRepository for Unwritable {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        self.0.load().await
    }

    async fn save(&self, _: &Settings) -> Result<(), RepositoryError> {
        Err(RepositoryError::Storage(
            "this store refuses writes".to_owned(),
        ))
    }
}

/// Machine A's pairing as `join` read it before the dial: no model
/// remembered yet, and no port.
fn held_for_a() -> RemotePairing {
    RemotePairing {
        ticket: TICKET_A.to_owned(),
        api_key: KEY_A.to_owned(),
        default_model: None,
        port: None,
    }
}

/// The stored pairing, if there is one.
async fn stored(core: &AppCore) -> Option<RemotePairing> {
    core.settings()
        .get()
        .await
        .expect("settings load")
        .remote_pairing
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
/// The store refuses every write, so the dial succeeding is what shows that
/// none was attempted.
#[tokio::test]
async fn a_dial_to_the_machine_already_recorded_writes_nothing() {
    let pool = setup_test_database().await.expect("in-memory DB");
    let mut repos = CoreFactory::build_repos(pool);
    repos.settings = Arc::new(Unwritable(Arc::clone(&repos.settings)));
    let core = AppCore::new(repos);
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
    .expect("a dial that needs no write does not fail on a store that refuses one");

    assert!(!paired, "no code was redeemed, so nothing was paired");
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
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("machine A's pairing is stored");
    let held = held_for_a();

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

/// A model remembered while a redial was under way survives the redial.
///
/// `held` is the record as `join` read it before the dial, with no model
/// in it; the model is remembered on the stored record after that read, the
/// way a `--remote` turn in a terminal would. The redial writes the ticket
/// and the port it used and leaves the rest of the record as it now is.
#[tokio::test]
async fn a_model_remembered_during_the_redial_survives_it() {
    let core = test_core().await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("machine A's pairing is stored");
    let held = held_for_a();
    remember_a_model(&core, "qwen3-coder").await;

    settle(
        &core,
        &ticket(TICKET_A_MOVED),
        Some(&held),
        None,
        8181,
        never_redeems,
    )
    .await
    .expect("the redial writes the new ticket");

    let stored = stored(&core).await.expect("a pairing is stored");
    assert_eq!(
        stored.default_model.as_deref(),
        Some("qwen3-coder"),
        "the redial wrote back the record it read before the dial"
    );
    assert_eq!(stored.ticket, ticket(TICKET_A_MOVED).to_string());
    assert_eq!(stored.port, Some(8181));
}

/// A pairing cleared while a redial was under way stays cleared.
#[tokio::test]
async fn a_pairing_cleared_during_the_redial_is_not_brought_back() {
    let core = test_core().await;

    settle(
        &core,
        &ticket(TICKET_A_MOVED),
        Some(&held_for_a()),
        None,
        8181,
        never_redeems,
    )
    .await
    .expect("a redial with nothing to write is not a failure");

    assert_eq!(
        stored(&core).await,
        None,
        "the redial brought the pairing back"
    );
}

/// A pairing with another machine, stored while a redial to machine A was
/// under way, keeps that machine's ticket beside that machine's key.
#[tokio::test]
async fn a_pairing_with_another_machine_during_the_redial_is_left_alone() {
    let core = test_core().await;
    core.settings()
        .update(paired_with(TICKET_B, KEY_B))
        .await
        .expect("machine B's pairing is stored");

    settle(
        &core,
        &ticket(TICKET_A_MOVED),
        Some(&held_for_a()),
        None,
        8181,
        never_redeems,
    )
    .await
    .expect("a redial with nothing to write is not a failure");

    let stored = stored(&core).await.expect("a pairing is stored");
    assert_eq!(
        stored.ticket, TICKET_B,
        "machine B's pairing was overwritten by the redial to machine A"
    );
    assert_eq!(stored.api_key, KEY_B);
    assert_eq!(
        stored.port, None,
        "machine A's port was filed on machine B's record"
    );
}
