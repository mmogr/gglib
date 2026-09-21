//! Tests for [`NotOpened::into_error`](super::NotOpened::into_error) and
//! [`parse_pairing`](super::parse_pairing): the sentences a dial or a
//! pairing that did not work ends in, and what a pasted pairing string is
//! forgiven.
//!
//! Beside the module rather than inside it, as most of `remote/` is
//! written — fourteen of the sixteen other tested files here declare a
//! sibling, and `connect_dial.rs` and `teardown.rs` do not.
//! `connect_open.rs` sat at the 300-line budget
//! `scripts/check_rust_complexity.sh` enforces, with its tests taking a
//! quarter of it.

use super::*;
use crate::test_support_remote::TICKET_A;

/// The two sentences a dial that reached nobody ends in, which
/// `docs/remote.md`'s troubleshooting table quotes: the wait names the
/// budget it was given, so a dial with a code says twenty-five.
#[test]
fn a_dial_that_reached_nobody_says_which_way_and_how_long_it_waited() {
    let GuiError::Unavailable(waited) = unreached(Unreached::TimedOut(REACH_WITHIN)) else {
        panic!("a wait that ran out is the machine being unavailable");
    };
    assert!(
        waited.starts_with("the remote machine did not answer within 25 seconds"),
        "{waited}"
    );
    let GuiError::Unavailable(closed) = unreached(Unreached::Closed(None)) else {
        panic!("a pipe that closed first is unavailable too");
    };
    assert!(
        closed.starts_with("the tunnel closed before the remote machine answered"),
        "{closed}"
    );
}

/// What a pairing that did not pair says: a refused code is most likely a
/// mistyped one, which costs only that attempt; a 404 is what a desktop on
/// gglib 0.18 sends back; and every other non-answer is unexplained, so it
/// must not blame a version.
#[test]
fn a_pairing_that_did_not_pair_says_what_to_do_next() {
    let GuiError::ValidationFailed(refused) = NotOpened::Pair(PairError::Refused).into_error(None)
    else {
        panic!("a refused code is the caller's to fix");
    };
    assert!(
        refused.starts_with("the far machine refused the pairing code — it was mistyped"),
        "{refused}"
    );
    let GuiError::Unavailable(old) =
        NotOpened::Pair(PairError::UnexpectedStatus { status: 404 }).into_error(None)
    else {
        panic!("an answer that is not a pairing answer leaves the far machine unavailable");
    };
    assert!(old.contains("a desktop on gglib 0.18 or older"), "{old}");
    // The half of the remedy this arm exists to restore: without it the
    // operator is told to update a desktop and not that the code is spent.
    assert!(old.contains("gglib remote invite"), "{old}");

    // The arm that exists so the one above cannot over-reach. Driven with
    // *two* statuses on purpose: with one, an arm that hard-coded that same
    // number would satisfy the assertion, and the test could not tell an
    // interpolated status from a constant. Neither number means anything —
    // that is the point of the arm — and 503 is deliberately not one of
    // them, because it is the example this change argues cannot arise.
    // Three separate assertions rather than one conjunction, so a mutation
    // that breaks one of them says which.
    for status in [429u16, 500] {
        let GuiError::Unavailable(unexplained) =
            NotOpened::Pair(PairError::UnexpectedStatus { status }).into_error(None)
        else {
            panic!("any status that is not a pairing answer leaves the far machine unavailable");
        };
        assert!(
            unexplained.contains(&status.to_string()),
            "it names the status it was given: {unexplained}"
        );
        assert!(
            !unexplained.contains("gglib 0.18"),
            "only the 404 has been traced to an old desktop: {unexplained}"
        );
        assert!(
            unexplained.contains("may have been spent"),
            "the code may be spent whatever the status: {unexplained}"
        );
    }

    // Still produced by modelpipe, and still reaching the arm that carries no
    // status: this one is not dead, and did not move with the 404.
    let GuiError::Unavailable(other) =
        NotOpened::Pair(PairError::Unexpected("an empty key or device")).into_error(None)
    else {
        panic!("any answer that is not a pairing answer leaves the far machine unavailable");
    };
    assert!(
        !other.contains("gglib 0.18") && other.contains("may have been spent"),
        "{other}"
    );
}

/// A pasted string is trimmed as `str::trim` trims, so a no-break space a
/// chat app left at either end is not a refusal; and a string with no code
/// that is not a ticket is called that, not "the part before the code".
#[test]
fn a_pasted_string_loses_any_whitespace_and_a_bad_bare_ticket_is_named_as_one() {
    let pasted = format!("\u{a0}{TICKET_A}-483920\u{3000}");
    let pairing = parse_pairing(&pasted).expect("whitespace at either end is let go");
    assert_eq!(
        pairing.code().map(modelpipe::PairingCode::as_str),
        Some("483920")
    );

    let Err(GuiError::ValidationFailed(bare)) = parse_pairing("pipenotaticket") else {
        panic!("a string that is not a ticket is the caller's to fix");
    };
    assert!(bare.starts_with("that is not a ticket: "), "{bare}");
    let Err(GuiError::ValidationFailed(coded)) = parse_pairing("pipenotaticket-483920") else {
        panic!("and so is one with a code");
    };
    assert!(
        coded.starts_with("the part before the code is not a ticket: "),
        "{coded}"
    );
}
