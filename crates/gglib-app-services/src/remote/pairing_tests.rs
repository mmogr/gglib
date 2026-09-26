//! What the open-invite slot promises, over an invite no listener minted.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use modelpipe::{InviteOutcome, PeerId};

use super::*;

/// An invite with nothing behind it: it ends when a test says so, and
/// withdrawing it behaves as modelpipe's does, a no-op once it has ended.
#[derive(Default)]
pub(crate) struct FakeInvite {
    outcome: Mutex<Option<InviteOutcome>>,
    withdrawn: AtomicBool,
}

impl FakeInvite {
    pub(crate) fn new() -> Arc<Self> {
        Arc::default()
    }

    /// End the invite, unless it already has.
    pub(crate) fn end(&self, outcome: InviteOutcome) {
        self.outcome.lock().unwrap().get_or_insert(outcome);
    }

    /// End it as a device redeeming the code from [`peer`], calling itself
    /// `label`.
    pub(crate) fn redeem(&self, device: &str, label: Option<&str>) {
        self.end(InviteOutcome::Redeemed {
            device: device.to_owned(),
            peer: peer(),
            label: label.map(str::to_owned),
        });
    }

    pub(crate) fn was_withdrawn(&self) -> bool {
        self.withdrawn.load(Ordering::Relaxed)
    }
}

impl Invitation for Arc<FakeInvite> {
    fn ended(&self) -> Option<InviteOutcome> {
        self.outcome.lock().unwrap().clone()
    }

    fn withdraw(&self) {
        self.withdrawn.store(true, Ordering::Relaxed);
        self.end(InviteOutcome::Withdrawn);
    }
}

/// The endpoint every fake redemption comes from.
pub(crate) fn peer() -> PeerId {
    "3c".repeat(32).parse().expect("sixty-four hex digits")
}

#[test]
fn an_invite_is_redeemable_until_it_ends() {
    let pairing = Pairing::default();
    let invite = FakeInvite::new();
    assert!(!pairing.active(), "nothing is open yet");

    pairing.begin("dev-0a1b2c3d".to_owned(), Box::new(Arc::clone(&invite)));
    assert!(pairing.active());

    invite.end(InviteOutcome::Expired);
    assert!(!pairing.active(), "an expired code is not redeemable");
}

/// Reading whether a code is live must not throw away a redemption nobody has
/// recorded yet: the device would pair and never be listed.
#[test]
fn asking_whether_an_invite_is_live_never_takes_it_out() {
    let pairing = Pairing::default();
    let invite = FakeInvite::new();
    pairing.begin("dev-0a1b2c3d".to_owned(), Box::new(Arc::clone(&invite)));

    invite.redeem("dev-0a1b2c3d", Some("Matt's iPhone"));
    assert!(!pairing.active());
    assert!(!pairing.active(), "asked twice, as a status poll does");

    let open = pairing.take().expect("still there for whoever records it");
    assert_eq!(open.device, "dev-0a1b2c3d");
    assert!(matches!(
        open.invitation.ended(),
        Some(InviteOutcome::Redeemed { .. })
    ));
}

/// Retiring one device leaves a code for another alone.
#[test]
fn taking_an_invite_for_one_device_leaves_anothers_in_place() {
    let pairing = Pairing::default();
    pairing.begin("dev-0a1b2c3d".to_owned(), Box::new(FakeInvite::new()));

    assert!(pairing.take_if("dev-4e5f6a7b").is_none());
    assert!(pairing.active(), "the phone's code is still live");
    assert!(pairing.take_if("dev-0a1b2c3d").is_some());
    assert!(!pairing.active());
}

/// A fake that withdraws after ending keeps the ending, as modelpipe's does,
/// so tests built on it cannot pass on an outcome the real invite would not
/// report.
#[test]
fn the_fake_keeps_the_first_ending_as_modelpipe_does() {
    let invite = FakeInvite::new();
    invite.redeem("dev-0a1b2c3d", None);
    invite.withdraw();
    assert!(invite.was_withdrawn());
    assert!(matches!(
        invite.ended(),
        Some(InviteOutcome::Redeemed { .. })
    ));
}

/// `docs/remote.md` promises two minutes, and three wrong codes from any one
/// endpoint before the edge locks that endpoint out.
#[test]
fn the_code_lives_two_minutes_and_locks_an_endpoint_out_after_three_wrong_codes() {
    assert_eq!(PAIRING_TTL, Duration::from_mins(2));
    assert_eq!(MAX_ATTEMPTS_AT_EDGE.get(), 3);
}
