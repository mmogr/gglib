//! Tests for [`super::RemoteGateway`] — the port as the proxy sees it, and the
//! session it holds. The invite a session holds is `gateway_invite_tests.rs`,
//! which shares the fixture below.

use std::sync::{Arc, Mutex};

use gglib_core::events::AppEvent;
use gglib_core::ports::{AppEventEmitter, RemoteGatewayPort};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::*;
use crate::remote::pairing::pairing_tests::FakeInvite;
use crate::remote::roster::Note;

#[derive(Default)]
pub(super) struct Recording(Mutex<Vec<AppEvent>>);

impl AppEventEmitter for Recording {
    fn emit(&self, event: AppEvent) {
        self.0.lock().unwrap().push(event);
    }
}

pub(super) fn gateway() -> (Arc<Recording>, RemoteGateway) {
    let recorder = Arc::new(Recording::default());
    let gateway = RemoteGateway::new(recorder.clone());
    (recorder, gateway)
}

/// The roster channel a session installs, so a test can read what was noted.
pub(super) fn notes(gateway: &RemoteGateway) -> UnboundedReceiver<Note> {
    let (sender, inbox) = unbounded_channel();
    gateway.take_notes(sender);
    inbox
}

/// Every `remote_paired` announced, by the peer it named.
pub(super) fn announced(events: &Recording) -> Vec<Option<String>> {
    events
        .0
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            AppEvent::RemotePaired { peer } => Some(peer.clone()),
            _ => None,
        })
        .collect()
}

/// Begin a session and open an invite for `device` on it, which is what an
/// `enable` followed by an `invite` does. The two are separate calls in the
/// real path because `enable` is a switch and `invite` pairs a device; these
/// tests are about what happens once something is open.
pub(super) fn arm_with(
    gateway: &RemoteGateway,
    device: &str,
    allow_mcp: bool,
) -> (u64, Arc<FakeInvite>) {
    let epoch = gateway.begin_session(allow_mcp);
    let invite = FakeInvite::new();
    let offered = gateway.offer_pairing(epoch, device.to_owned(), Box::new(Arc::clone(&invite)));
    assert_eq!(offered, Offered::Armed, "the fixture must arm");
    (epoch, invite)
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
    let (epoch, invite) = arm_with(&gateway, "dev-0a1b2c3d", true);
    invite.redeem("dev-0a1b2c3d", None);
    gateway.settle_invite("dev-0a1b2c3d");
    gateway.note_tunnelled_request(None, None);
    // The pairing counts as the request it was, as it did while the proxy
    // answered it, and so does the request after it.
    assert_eq!(gateway.tunnelled_requests(), 2);

    gateway.reset_session_if(epoch);
    assert!(!gateway.mcp_allowed());
    assert!(!gateway.paired());
    assert!(!gateway.pairing.active());
    assert_eq!(gateway.tunnelled_requests(), 2, "history survives");
}

/// The epoch is what a slow teardown presents to prove the session it is
/// ending is still the one that is armed. A teardown drains for up to five
/// seconds without the `live` lock, so a whole `enable` can land inside it,
/// and the stale reset must be a no-op rather than a wipe.
#[test]
fn a_reset_for_a_superseded_session_leaves_the_current_one_alone() {
    let (_, gateway) = gateway();
    let (first, first_invite) = arm_with(&gateway, "dev-0a1b2c3d", true);
    let (second, second_invite) = arm_with(&gateway, "dev-4e5f6a7b", true);
    assert_ne!(first, second, "each session gets its own epoch");
    assert!(
        first_invite.was_withdrawn(),
        "a new session inherits no invite"
    );

    gateway.reset_session_if(first);
    assert!(gateway.mcp_allowed(), "the second session's grant stands");
    assert!(gateway.pairing.active(), "and so does its code");
    assert!(!second_invite.was_withdrawn());
}

/// Arming a session says nobody has paired with it yet. It has to, because
/// the previous session's teardown may never clear anything — it declines
/// the moment this one takes the gateway over — and `status` would otherwise
/// report the last session's answer for a tunnel nobody has reached.
#[test]
fn arming_a_session_starts_it_unpaired() {
    let (_, gateway) = gateway();
    let (_, invite) = arm_with(&gateway, "dev-0a1b2c3d", false);
    invite.redeem("dev-0a1b2c3d", None);
    gateway.settle_invite("dev-0a1b2c3d");
    assert!(gateway.paired());

    arm_with(&gateway, "dev-4e5f6a7b", false);
    assert!(!gateway.paired());
}

/// A second code on a live session says nobody has taken *it* yet.
///
/// `paired` used to move only when a session began or ended, so one device
/// pairing left it true for the rest of the session. `enable --invite`
/// against a tunnel that is already up — the path that exists so pairing a
/// second device costs nobody else their connection — would then have the
/// pairing screen break out on the first device's answer and stop watching,
/// while the second code stayed redeemable unwatched.
#[test]
fn offering_a_second_code_says_nobody_has_taken_it_yet() {
    let (_, gateway) = gateway();
    let (epoch, invite) = arm_with(&gateway, "dev-0a1b2c3d", false);
    invite.redeem("dev-0a1b2c3d", None);
    gateway.settle_invite("dev-0a1b2c3d");
    assert!(gateway.paired());

    let offered = gateway.offer_pairing(
        epoch,
        "dev-4e5f6a7b".to_owned(),
        Box::new(FakeInvite::new()),
    );
    assert_eq!(offered, Offered::Armed, "the session is still the live one");
    assert!(
        !gateway.paired(),
        "the first device's answer is not an answer for this code"
    );
}

/// A session begun without a code opens none, and withdraws whatever the last
/// one left. This is what a restart does: the daemon puts the tunnel back
/// because the switch says to, and nobody is watching for a pairing string.
#[test]
fn a_session_begun_without_a_code_holds_none_and_withdraws_the_last() {
    let (_, gateway) = gateway();
    let (_, invite) = arm_with(&gateway, "dev-0a1b2c3d", false);
    assert!(gateway.pairing.active(), "the armed session has a code");

    gateway.begin_session(false);

    assert!(
        !gateway.pairing.active(),
        "a session begun without a code has none redeemable"
    );
    assert!(
        invite.was_withdrawn(),
        "and the previous session's code is withdrawn, not inherited"
    );
}

#[test]
fn debug_reports_state_only() {
    let (_, gateway) = gateway();
    arm_with(&gateway, "dev-0a1b2c3d", false);
    let rendered = format!("{gateway:?}");
    assert!(rendered.contains("pairing_active: true"), "{rendered}");
}

/// A session that ended takes its name with it, so nothing is held open
/// against it afterwards.
///
/// The other half of the epoch guard: the tests above cover a teardown
/// *superseded by a newer session*, where the epochs differ because
/// `begin_session` moved the counter. With no successor nothing moved it, so
/// a dead epoch that still matched would let `offer_pairing` hold an invite
/// for a tunnel that is down, which `status` would report as a live code.
///
/// The window is real rather than theoretical: `invite` reads the epoch under
/// the serve slot, releases it, then mints a key and writes two stores before
/// it comes back here.
#[test]
fn a_code_cannot_be_offered_against_a_session_that_has_ended() {
    let (_recorder, gateway) = gateway();
    let epoch = gateway.begin_session(true);

    // The teardown, with nothing taking the tunnel's place.
    gateway.reset_session_if(epoch);

    assert_eq!(
        gateway.offer_pairing(
            epoch,
            "dev-0a1b2c3d".to_owned(),
            Box::new(FakeInvite::new()),
        ),
        Offered::Superseded,
        "the epoch a dead session was armed under must stop matching"
    );
    assert!(
        !gateway.pairing.active(),
        "and nothing is redeemable against a tunnel that is down"
    );
}
