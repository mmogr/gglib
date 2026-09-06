//! Tests for [`super::Pairing`] — the one-code, one-redemption contract.

use std::time::Duration;

use gglib_core::ports::PairingOutcome;

use super::*;

const CODE: &str = "483920";
const KEY: &str = "sk-zzq-the-real-key";

fn armed() -> Pairing {
    let pairing = Pairing::default();
    pairing.begin(CODE.to_owned(), KEY.to_owned(), PAIRING_TTL);
    pairing
}

#[test]
fn the_right_code_is_granted_once_and_then_rejected() {
    let pairing = armed();
    assert_eq!(
        pairing.redeem(CODE),
        PairingOutcome::Granted(KEY.to_owned())
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
        PairingOutcome::Granted(KEY.to_owned())
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
    pairing.begin(CODE.to_owned(), KEY.to_owned(), Duration::ZERO);
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

#[test]
fn a_rotation_during_pairing_hands_out_the_new_key() {
    let pairing = armed();
    pairing.update_key("sk-zzq-rotated".to_owned());
    assert_eq!(
        pairing.redeem(CODE),
        PairingOutcome::Granted("sk-zzq-rotated".to_owned())
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
    pairing.begin("111111".to_owned(), "other-key".to_owned(), PAIRING_TTL);
    assert_eq!(pairing.redeem(CODE), PairingOutcome::Rejected, "old code");
    assert_eq!(
        pairing.redeem("111111"),
        PairingOutcome::Granted("other-key".to_owned())
    );
}
