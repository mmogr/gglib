//! Tests for [`super::RemoteGateway`] — the port as the proxy sees it.

use std::sync::{Arc, Mutex};

use gglib_core::events::AppEvent;
use gglib_core::ports::{AppEventEmitter, PairingOutcome, RemoteGatewayPort};

use super::*;
use crate::remote::pairing::PAIRING_TTL;

#[derive(Default)]
struct Recording(Mutex<Vec<AppEvent>>);

impl AppEventEmitter for Recording {
    fn emit(&self, event: AppEvent) {
        self.0.lock().unwrap().push(event);
    }
}

fn gateway() -> (Arc<Recording>, RemoteGateway) {
    let recorder = Arc::new(Recording::default());
    let gateway = RemoteGateway::new(recorder.clone());
    (recorder, gateway)
}

/// Begin a session and offer a code on it, which is what an `enable` followed
/// by an `invite` does. The two are separate calls in the real path because
/// `enable` is a switch and `invite` pairs a device; these tests are about
/// what happens once something is armed.
pub(super) fn arm_with(gateway: &RemoteGateway, code: &str, key: &str, allow_mcp: bool) -> u64 {
    let epoch = gateway.begin_session(allow_mcp);
    let offered = gateway.offer_pairing(
        epoch,
        code.to_owned(),
        key.to_owned(),
        "dev-0a1b2c3d".to_owned(),
        PAIRING_TTL,
    );
    assert_eq!(offered, Offered::Armed, "the fixture must arm");
    epoch
}

#[test]
fn a_granted_code_marks_the_session_paired_and_says_which_peer() {
    let (events, gateway) = gateway();
    gateway.pairing.begin_for(
        "483920".to_owned(),
        "the-key".to_owned(),
        "dev-0a1b2c3d".to_owned(),
        PAIRING_TTL,
    );
    assert!(!gateway.paired());

    let outcome = gateway.redeem_pairing_code("483920", Some("3ca82708b995"), None);
    assert_eq!(
        outcome,
        PairingOutcome::Granted {
            key: "the-key".to_owned(),
            device: "dev-0a1b2c3d".to_owned(),
        }
    );
    assert!(gateway.paired());

    let recorded = events.0.lock().unwrap();
    assert!(
        matches!(&recorded[..], [AppEvent::RemotePaired { peer: Some(p) }] if p == "3ca82708b995"),
        "{recorded:?}"
    );
}

#[test]
fn a_rejected_code_emits_nothing_and_pairs_nobody() {
    let (events, gateway) = gateway();
    gateway.pairing.begin_for(
        "483920".to_owned(),
        "the-key".to_owned(),
        "dev-0a1b2c3d".to_owned(),
        PAIRING_TTL,
    );
    assert_eq!(
        gateway.redeem_pairing_code("000000", None, None),
        PairingOutcome::Rejected
    );
    assert!(!gateway.paired());
    assert!(events.0.lock().unwrap().is_empty());
}

#[test]
fn tunnelled_requests_are_counted_and_the_last_peer_remembered() {
    let (_, gateway) = gateway();
    assert_eq!(gateway.tunnelled_requests(), 0);
    assert_eq!(gateway.last_tunnelled_ms(), None);
    assert_eq!(gateway.last_peer(), None);

    gateway.note_tunnelled_request(Some("aaaaaaaaaaaa"), None);
    gateway.note_tunnelled_request(None, None);
    assert_eq!(gateway.tunnelled_requests(), 2);
    assert!(gateway.last_tunnelled_ms().is_some());
    assert_eq!(gateway.last_peer().as_deref(), Some("aaaaaaaaaaaa"));
}

#[test]
fn resetting_the_session_keeps_the_history() {
    let (_, gateway) = gateway();
    let epoch = arm_with(&gateway, "483920", "the-key", true);
    gateway.redeem_pairing_code("483920", None, None);
    gateway.note_tunnelled_request(None, None);

    gateway.reset_session_if(epoch);
    assert!(!gateway.mcp_allowed());
    assert!(!gateway.paired());
    assert!(!gateway.pairing.active());
    assert_eq!(gateway.tunnelled_requests(), 1, "history survives");
}

/// The epoch is what a slow teardown presents to prove the session it is
/// ending is still the one that is armed. A teardown drains for up to five
/// seconds without the `live` lock, so a whole `enable` can land inside it,
/// and the stale reset must be a no-op rather than a wipe.
#[test]
fn a_reset_for_a_superseded_session_leaves_the_current_one_alone() {
    let (_, gateway) = gateway();
    let first = arm_with(&gateway, "483920", "the-key", true);
    let second = arm_with(&gateway, "111111", "the-next-key", true);
    assert_ne!(first, second, "each session gets its own epoch");

    gateway.reset_session_if(first);
    assert!(gateway.mcp_allowed(), "the second session's grant stands");
    assert_eq!(
        gateway.redeem_pairing_code("111111", None, None),
        PairingOutcome::Granted {
            key: "the-next-key".to_owned(),
            device: "dev-0a1b2c3d".to_owned(),
        },
        "and so does its code"
    );
}

/// Arming a session says nobody has paired with it yet. It has to, because
/// the previous session's teardown may never clear anything — it declines
/// the moment this one takes the gateway over — and `status` would otherwise
/// report the last session's answer for a tunnel nobody has reached.
#[test]
fn arming_a_session_starts_it_unpaired() {
    let (_, gateway) = gateway();
    arm_with(&gateway, "483920", "the-key", false);
    gateway.redeem_pairing_code("483920", None, None);
    assert!(gateway.paired());

    arm_with(&gateway, "111111", "the-next-key", false);
    assert!(!gateway.paired());
}

/// A session begun without a code arms none, and clears whatever the last
/// one left. This is what a restart does: the daemon puts the tunnel back
/// because the switch says to, and nobody is watching for a pairing string.
/// Inheriting the previous session's code would leave one redeemable that
/// no person ever saw, for two minutes, at every boot.
#[test]
fn a_session_begun_without_a_code_arms_none_and_clears_the_last() {
    let (_, gateway) = gateway();
    arm_with(&gateway, "483920", "the-key", false);
    assert!(gateway.pairing.active(), "the armed session has a code");

    gateway.begin_session(false);

    assert!(
        !gateway.pairing.active(),
        "a session begun without a code has none redeemable"
    );
    assert!(
        matches!(
            gateway.redeem_pairing_code("483920", None, None),
            PairingOutcome::Rejected
        ),
        "and the previous session's code is not inherited"
    );
}

#[test]
fn debug_reports_state_and_never_the_code_or_key() {
    let (_, gateway) = gateway();
    gateway.pairing.begin_for(
        "483920".to_owned(),
        "sk-zzq-secret".to_owned(),
        "dev-0a1b2c3d".to_owned(),
        PAIRING_TTL,
    );
    let rendered = format!("{gateway:?}");
    assert!(!rendered.contains("483920"), "{rendered}");
    assert!(!rendered.contains("sk-zzq-secret"), "{rendered}");
    assert!(rendered.contains("pairing_active: true"), "{rendered}");
}
