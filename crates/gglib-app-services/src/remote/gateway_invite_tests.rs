//! Tests for the invite a gateway session holds: how a device that redeemed
//! is recorded, and that it is recorded exactly once whoever takes the invite
//! out of the slot first.
//!
//! Split from `gateway_tests.rs`, which is about the session and its epoch
//! and would cross its size budget with these in it. They share its fixture.

use modelpipe::InviteOutcome;

use super::gateway_tests::{announced, arm_with, gateway, notes};
use super::*;
use crate::remote::pairing::pairing_tests::{FakeInvite, peer};
use crate::remote::roster::Note;

/// A device that redeemed its code marks the session paired, is announced
/// with the endpoint it came from, is noted for the roster with its label and
/// that endpoint, and is counted in `status` as the request it was.
#[test]
fn a_redeemed_invite_pairs_the_session_and_records_the_device_and_its_endpoint() {
    let (events, gateway) = gateway();
    let mut inbox = notes(&gateway);
    let (_, invite) = arm_with(&gateway, "dev-0a1b2c3d", false);
    assert!(!gateway.paired());

    invite.redeem("dev-0a1b2c3d", Some("Matt's iPhone"));
    gateway.settle_invite("dev-0a1b2c3d");

    assert!(gateway.paired());
    assert!(!gateway.pairing.active(), "the code is spent");
    let fingerprint = peer().fingerprint();
    assert_eq!(announced(&events), [Some(fingerprint.clone())]);
    assert_eq!(
        (gateway.tunnelled_requests(), gateway.last_peer()),
        (1, Some(fingerprint.clone())),
        "the pairing is counted as the request it was, and its endpoint is the last peer"
    );
    let Ok(Note::Joined {
        device,
        label,
        peer: noted,
        ..
    }) = inbox.try_recv()
    else {
        panic!("the join was not noted for the roster");
    };
    assert_eq!(
        (device.as_str(), label.as_deref(), noted.as_deref()),
        (
            "dev-0a1b2c3d",
            Some("Matt's iPhone"),
            Some(fingerprint.as_str())
        )
    );
}

#[test]
fn an_invite_that_ended_unredeemed_pairs_nobody_and_announces_nothing() {
    let (events, gateway) = gateway();
    let mut inbox = notes(&gateway);
    let (_, invite) = arm_with(&gateway, "dev-0a1b2c3d", false);

    invite.end(InviteOutcome::Expired);
    gateway.settle_invite("dev-0a1b2c3d");

    assert!(!gateway.paired());
    assert!(announced(&events).is_empty());
    assert!(inbox.try_recv().is_err(), "nothing happened to note");
}

/// A code burned by guessers pairs nobody and announces nothing; its row
/// stays, listed as never joined.
#[test]
fn a_burned_invite_pairs_nobody_and_announces_nothing() {
    let (events, gateway) = gateway();
    let mut inbox = notes(&gateway);
    let (_, invite) = arm_with(&gateway, "dev-0a1b2c3d", false);

    invite.end(InviteOutcome::Burned);
    gateway.settle_invite("dev-0a1b2c3d");

    assert!(!gateway.paired());
    assert!(!gateway.pairing.active(), "a burned code is not redeemable");
    assert!(announced(&events).is_empty());
    assert!(inbox.try_recv().is_err(), "nothing happened to note");
}

/// The watcher is a task, and a new offer can take an ended invite out of the
/// slot before it runs. The device must still be recorded, and only once.
#[test]
fn a_redemption_the_watcher_has_not_recorded_yet_is_recorded_by_the_next_offer() {
    let (events, gateway) = gateway();
    let mut inbox = notes(&gateway);
    let (epoch, first) = arm_with(&gateway, "dev-0a1b2c3d", false);
    first.redeem("dev-0a1b2c3d", None);

    let offered = gateway.offer_pairing(
        epoch,
        "dev-4e5f6a7b".to_owned(),
        Box::new(FakeInvite::new()),
    );
    // The watcher, arriving late.
    gateway.settle_invite("dev-0a1b2c3d");

    assert_eq!(
        offered,
        Offered::Armed,
        "an ended invite holds nothing open"
    );
    assert!(
        matches!(inbox.try_recv(), Ok(Note::Joined { device, .. }) if device == "dev-0a1b2c3d"),
        "the first device was recorded"
    );
    assert!(
        inbox.try_recv().is_err(),
        "once, and not again by the watcher"
    );
    assert_eq!(announced(&events).len(), 1);
    assert!(!gateway.paired(), "the new code has not been taken");
}

/// The same race at a teardown: the reset takes the invite out, and records
/// the device before it lets the roster's writer go.
#[test]
fn a_redemption_the_watcher_has_not_recorded_yet_is_recorded_by_the_reset() {
    let (events, gateway) = gateway();
    let mut inbox = notes(&gateway);
    let (epoch, invite) = arm_with(&gateway, "dev-0a1b2c3d", false);
    invite.redeem("dev-0a1b2c3d", None);

    gateway.reset_session_if(epoch);

    assert!(
        matches!(inbox.try_recv(), Ok(Note::Joined { device, .. }) if device == "dev-0a1b2c3d"),
        "the join reached the writer before its channel closed"
    );
    assert_eq!(announced(&events).len(), 1);
    assert!(!gateway.paired(), "and the session is over");
}

/// One invite at a time is this machine's rule, not modelpipe's: a second
/// offer while a code is live is refused, and the live code is left alone.
#[test]
fn a_second_offer_while_a_code_is_live_is_refused_and_leaves_it_live() {
    let (_, gateway) = gateway();
    let (epoch, invite) = arm_with(&gateway, "dev-0a1b2c3d", false);

    let offered = gateway.offer_pairing(
        epoch,
        "dev-4e5f6a7b".to_owned(),
        Box::new(FakeInvite::new()),
    );

    assert_eq!(offered, Offered::AlreadyOpen);
    assert!(gateway.pairing.active());
    assert!(
        !invite.was_withdrawn(),
        "the code on screen stays redeemable"
    );
}
