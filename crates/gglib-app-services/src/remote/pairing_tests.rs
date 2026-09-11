//! Tests for [`super::Pairing`] — the one-code, one-redemption contract.

use std::time::Duration;

use gglib_core::ports::PairingOutcome;

use super::*;

const CODE: &str = "483920";
const KEY: &str = "sk-zzq-the-real-key";

fn armed() -> Pairing {
    let pairing = Pairing::default();
    pairing.begin_for(
        CODE.to_owned(),
        KEY.to_owned(),
        "dev-0a1b2c3d".to_owned(),
        PAIRING_TTL,
    );
    pairing
}

#[test]
fn the_right_code_is_granted_once_and_then_rejected() {
    let pairing = armed();
    assert_eq!(
        pairing.redeem(CODE),
        PairingOutcome::Granted {
            key: KEY.to_owned(),
            device: "dev-0a1b2c3d".to_owned(),
        }
    );
    assert_eq!(pairing.redeem(CODE), PairingOutcome::Rejected, "spent");
    assert!(!pairing.active());
}

#[test]
fn a_wrong_code_is_rejected_and_the_right_one_still_works_within_the_budget() {
    let pairing = armed();
    assert_eq!(pairing.redeem("000000"), PairingOutcome::Rejected);
    assert_eq!(pairing.redeem("483921"), PairingOutcome::Rejected);
    assert!(pairing.active(), "two misses leave it armed");
    assert_eq!(
        pairing.redeem(CODE),
        PairingOutcome::Granted {
            key: KEY.to_owned(),
            device: "dev-0a1b2c3d".to_owned(),
        }
    );
}

#[test]
fn the_third_wrong_code_burns_the_pairing() {
    let pairing = armed();
    for _ in 0..MAX_ATTEMPTS {
        assert_eq!(pairing.redeem("000000"), PairingOutcome::Rejected);
    }
    assert!(!pairing.active(), "burned");
    assert_eq!(
        pairing.redeem(CODE),
        PairingOutcome::Rejected,
        "the right code is dead too"
    );
}

#[test]
fn an_expired_code_is_rejected_whatever_is_presented() {
    let pairing = Pairing::default();
    pairing.begin_for(
        CODE.to_owned(),
        KEY.to_owned(),
        "dev-0a1b2c3d".to_owned(),
        Duration::ZERO,
    );
    assert!(!pairing.active());
    assert_eq!(pairing.redeem(CODE), PairingOutcome::Rejected);
}

#[test]
fn nothing_pending_rejects_everything() {
    let pairing = Pairing::default();
    assert_eq!(pairing.redeem(CODE), PairingOutcome::Rejected);
    assert_eq!(pairing.redeem(""), PairingOutcome::Rejected);
}

/// `redeem` compares the bytes it is handed and forgives nothing: a prefix,
/// a suffix, a different length and a stray space all lose, and none of them
/// can be told apart from the outside.
///
/// **A claim about this function, not about the route.** The only production
/// caller — `gglib-proxy`'s `POST /v1/remote/pair` — trims the presented code
/// before it arrives here, deliberately, so end to end a code pasted with
/// whitespace around it *is* accepted. That is paste-safety and not a wider
/// code space: `"483920 "` and `"483920"` name the same six digits, so the
/// three-attempt burn still covers the same twenty bits. Keeping the trim at
/// the edge and strictness here is what leaves one place to read for each.
/// The other half is pinned by
/// `whitespace_around_a_pasted_code_is_trimmed_not_refused` in
/// `gglib-proxy/tests/integration_remote_pair.rs`.
#[test]
fn redeem_compares_the_bytes_it_is_handed_and_trims_nothing() {
    for wrong in ["48392", "4839200", "", "483920 ", " 483920"] {
        let pairing = armed();
        assert_eq!(pairing.redeem(wrong), PairingOutcome::Rejected, "{wrong:?}");
    }
}

/// A pairing hands out the key it was armed with, and a rotation of the
/// *proxy's* key does not reach into it.
///
/// It used to: `Pairing::update_key` existed because a redemption handed out
/// `proxy_api_key`, so a rotation landing while the code was on screen had to
/// hand out the new one. A device now redeems for a key of its own, which no
/// rotation touches — and keeping that path would have overwritten the
/// device's key with the backend credential, giving the joining device the
/// one thing `backend_auth` exists to keep off it.
#[test]
fn a_pairing_hands_out_the_key_it_was_armed_with() {
    let pairing = armed();
    assert_eq!(
        pairing.redeem(CODE),
        PairingOutcome::Granted {
            key: KEY.to_owned(),
            device: "dev-0a1b2c3d".to_owned(),
        }
    );
}

#[test]
fn clearing_forgets_the_code() {
    let pairing = armed();
    pairing.clear();
    assert!(!pairing.active());
    assert_eq!(pairing.redeem(CODE), PairingOutcome::Rejected);
}

#[test]
fn re_arming_replaces_the_previous_code() {
    let pairing = armed();
    pairing.begin_for(
        "111111".to_owned(),
        "other-key".to_owned(),
        "dev-0a1b2c3d".to_owned(),
        PAIRING_TTL,
    );
    assert_eq!(pairing.redeem(CODE), PairingOutcome::Rejected, "old code");
    assert_eq!(
        pairing.redeem("111111"),
        PairingOutcome::Granted {
            key: "other-key".to_owned(),
            device: "dev-0a1b2c3d".to_owned(),
        }
    );
}

/// `withdraw_if` retires a code only when it belongs to the device named.
///
/// It exists for `forget`: a code still on screen for a device being retired
/// would otherwise redeem for a key the edge has just stopped holding — a
/// device that pairs, shows a green checkmark, and is refused on its first
/// real request. The `if` is the other half: retiring the laptop must not
/// cancel the code a person is at that moment typing into their phone.
#[test]
fn withdrawing_for_a_device_leaves_another_devices_code_alone() {
    let pairing = armed();
    assert_eq!(pairing.withdraw_if("dev-99887766"), None, "not its code");
    assert!(pairing.active(), "and it is still redeemable");

    assert_eq!(
        pairing.withdraw_if("dev-0a1b2c3d"),
        Some("dev-0a1b2c3d".to_owned())
    );
    assert!(!pairing.active());
    assert_eq!(pairing.redeem(CODE), PairingOutcome::Rejected);

    // Nothing pending is not an error, which is what lets `forget` call it
    // unconditionally.
    assert_eq!(pairing.withdraw_if("dev-0a1b2c3d"), None);
}
