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

#[test]
fn a_granted_code_marks_the_session_paired_and_says_which_peer() {
    let (events, gateway) = gateway();
    gateway
        .pairing
        .begin("483920".to_owned(), "the-key".to_owned(), PAIRING_TTL);
    assert!(!gateway.paired());

    let outcome = gateway.redeem_pairing_code("483920", Some("3ca82708b995"));
    assert_eq!(outcome, PairingOutcome::Granted("the-key".to_owned()));
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
    gateway
        .pairing
        .begin("483920".to_owned(), "the-key".to_owned(), PAIRING_TTL);
    assert_eq!(
        gateway.redeem_pairing_code("000000", None),
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

    gateway.note_tunnelled_request(Some("aaaaaaaaaaaa"));
    gateway.note_tunnelled_request(None);
    assert_eq!(gateway.tunnelled_requests(), 2);
    assert!(gateway.last_tunnelled_ms().is_some());
    assert_eq!(gateway.last_peer().as_deref(), Some("aaaaaaaaaaaa"));
}

#[test]
fn resetting_the_session_keeps_the_history() {
    let (_, gateway) = gateway();
    let epoch = gateway.begin_session("483920".to_owned(), "the-key".to_owned(), PAIRING_TTL, true);
    gateway.redeem_pairing_code("483920", None);
    gateway.note_tunnelled_request(None);

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
    let first = gateway.begin_session("483920".to_owned(), "the-key".to_owned(), PAIRING_TTL, true);
    let second = gateway.begin_session(
        "111111".to_owned(),
        "the-next-key".to_owned(),
        PAIRING_TTL,
        true,
    );
    assert_ne!(first, second, "each session gets its own epoch");

    gateway.reset_session_if(first);
    assert!(gateway.mcp_allowed(), "the second session's grant stands");
    assert_eq!(
        gateway.redeem_pairing_code("111111", None),
        PairingOutcome::Granted("the-next-key".to_owned()),
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
    gateway.begin_session(
        "483920".to_owned(),
        "the-key".to_owned(),
        PAIRING_TTL,
        false,
    );
    gateway.redeem_pairing_code("483920", None);
    assert!(gateway.paired());

    gateway.begin_session(
        "111111".to_owned(),
        "the-next-key".to_owned(),
        PAIRING_TTL,
        false,
    );
    assert!(!gateway.paired());
}

#[test]
fn debug_reports_state_and_never_the_code_or_key() {
    let (_, gateway) = gateway();
    gateway
        .pairing
        .begin("483920".to_owned(), "sk-zzq-secret".to_owned(), PAIRING_TTL);
    let rendered = format!("{gateway:?}");
    assert!(!rendered.contains("483920"), "{rendered}");
    assert!(!rendered.contains("sk-zzq-secret"), "{rendered}");
    assert!(rendered.contains("pairing_active: true"), "{rendered}");
}
