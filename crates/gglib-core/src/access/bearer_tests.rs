//! Tests for [`super::bearer_matches`] — what counts as presenting the key.

use super::bearer_matches;

const KEY: &str = "sk-secret123";

/// The case the change exists for. Every spelling of the scheme is the same
/// scheme, and a client that picks one is not sending a wrong credential.
#[test]
fn every_spelling_of_the_scheme_is_accepted() {
    for header in [
        "Bearer sk-secret123",
        "bearer sk-secret123",
        "BEARER sk-secret123",
        "BeArEr sk-secret123",
    ] {
        assert!(
            bearer_matches(Some(header), KEY),
            "{header:?} presents the key"
        );
    }
}

/// `1*SP` between the scheme and the credential, and trailing whitespace is
/// not part of the credential.
#[test]
fn the_separator_may_be_more_than_one_space() {
    assert!(bearer_matches(Some("Bearer  sk-secret123"), KEY));
    assert!(bearer_matches(Some("Bearer sk-secret123 "), KEY));
}

/// The grammar says `1*SP`, so a tab is not the separator. Rejected rather
/// than normalised: nothing sends this, and quietly accepting it would widen
/// the header shapes this endpoint claims to understand for no caller's sake.
#[test]
fn a_tab_is_not_the_separator() {
    assert!(!bearer_matches(Some("Bearer\tsk-secret123"), KEY));
}

/// Every flavour of a wrong credential a lenient comparison would let past.
/// These all failed before the change and must keep failing after it — the
/// point was to stop rejecting *correct* requests, not to start accepting
/// incorrect ones.
#[test]
fn every_flavour_of_wrong_credential_is_still_refused() {
    let cases: Vec<(&str, Option<&str>)> = vec![
        ("no Authorization header at all", None),
        ("a different key", Some("Bearer nope")),
        (
            "the right key under a different scheme",
            Some("Basic sk-secret123"),
        ),
        ("the key with no scheme", Some("sk-secret123")),
        ("a prefix of the key", Some("Bearer sk-secret")),
        ("the key plus a suffix", Some("Bearer sk-secret1234")),
        ("an empty header", Some("")),
        ("the scheme with no credential", Some("Bearer")),
        ("the scheme and a space, nothing more", Some("Bearer ")),
        ("the scheme and only whitespace", Some("Bearer    ")),
    ];

    for (why, header) in cases {
        assert!(
            !bearer_matches(header, KEY),
            "{why} must not authenticate: {header:?}"
        );
    }
}

/// A blank expectation cannot be satisfied by anything, including a blank
/// credential. Settings validation refuses to store one; this is the second
/// line, so a blank arriving by some other route fails closed rather than
/// admitting every caller.
#[test]
fn a_blank_expected_key_admits_nobody() {
    for header in [None, Some("Bearer "), Some("Bearer x"), Some("")] {
        assert!(
            !bearer_matches(header, ""),
            "a blank key must admit nobody: {header:?}"
        );
    }
}

/// The scheme is compared case-insensitively; the credential is not.
#[test]
fn the_credential_stays_case_sensitive() {
    assert!(!bearer_matches(Some("Bearer SK-SECRET123"), KEY));
    assert!(!bearer_matches(Some("bearer Sk-Secret123"), KEY));
}
