//! Tests for the copy `gglib remote disable` prints.
//!
//! A `#[path]` sibling rather than an inline `mod tests`, because the two
//! together crossed the 300-line budget — the same split `wire_tests.rs` and
//! `serve_switch.rs` were made along.

use super::*;

/// [#1005]: the notice may keep the half that is always true and must not
/// keep the half that is true of one bind only. Asserted on the constant
/// rather than on captured stderr because `disable` cannot be reached
/// without a daemon, and the string is the whole of what this fixes.
///
/// [#1005]: https://github.com/mmogr/gglib/issues/1005
#[test]
fn the_disable_notice_says_the_key_stays_without_claiming_it_can_never_go() {
    let notice = DISABLE_NOTICE.join(" ");

    assert!(
        notice.contains("The API key stays in settings"),
        "the part that holds for every bind is still said: {notice}"
    );
    assert!(
        !notice.contains("never off by itself"),
        "the unqualified claim is gone: {notice}"
    );
    assert!(
        notice.contains("depends"),
        "what replaced it names the bind it depends on: {notice}"
    );
    assert!(
        notice.contains("gglib config settings show"),
        "the operator is pointed at the state rather than left to infer it: {notice}"
    );
}

/// The identity lasts now (ADR 0012, decision 4, reversed), so `disable`
/// must not promise a fresh ticket on the next `enable` — it hands the
/// same one back. Saying otherwise sent people to `disable`/`enable` to
/// rotate an address that does not rotate.
#[test]
fn the_disable_notice_does_not_promise_a_new_ticket() {
    let notice = DISABLE_NOTICE.join(" ");

    assert!(
        !notice.contains("mints a new one"),
        "the claim that `enable` mints a fresh ticket is gone: {notice}"
    );
    assert!(
        notice.contains("the same one back"),
        "what replaced it says the ticket survives: {notice}"
    );
    assert!(
        notice.contains("deleting the endpoint key"),
        "and names what revoking actually is: {notice}"
    );
}

/// The banner is printed a line at a time, so a line that outgrew the
/// terminal would wrap into the two-space indent every other line carries.
#[test]
fn every_line_of_the_disable_notice_fits_a_narrow_terminal() {
    for line in DISABLE_NOTICE {
        assert!(
            line.starts_with("  "),
            "the banner's indent is part of the line: {line:?}"
        );
        assert!(
            line.chars().count() <= 80,
            "{} chars is past an 80-column terminal: {line:?}",
            line.chars().count()
        );
    }
}
