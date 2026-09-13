//! Tests for the copy `gglib remote enable` prints.
//!
//! A `#[path]` sibling rather than an inline `mod tests`, because the two
//! together would cross the 300-line budget, the split `disable.rs` and its
//! tests were made along.

use super::{INVITE_NOTICE, RESUMED_NOTICE};

/// An invite claims the code it offered, not a device that may never come:
/// the notice follows "expired and nobody paired" as readily as "paired",
/// so the old claim was false whenever nobody came.
#[test]
fn the_invite_notice_claims_a_code_and_not_a_device() {
    assert!(!INVITE_NOTICE.contains("added a device"), "{INVITE_NOTICE}");
    assert!(
        INVITE_NOTICE.contains("offered a code for one more device"),
        "{INVITE_NOTICE}"
    );
    assert!(
        INVITE_NOTICE.contains("changed nothing else about the session"),
        "{INVITE_NOTICE}"
    );
}

/// A plain `enable` that waited for the daemon's own resume was answered from
/// a session it did not arm. It says the session came back and changed
/// nothing, rather than that the switch was just thrown, which would bring the
/// key notice and `--allow-mcp` advice with it ([#1037]).
///
/// [#1037]: https://github.com/mmogr/gglib/issues/1037
#[test]
fn the_resumed_notice_says_the_session_came_back_and_changed_nothing() {
    assert!(
        RESUMED_NOTICE.contains("already coming back up"),
        "{RESUMED_NOTICE}"
    );
    assert!(
        RESUMED_NOTICE.contains("changed nothing about the session"),
        "{RESUMED_NOTICE}"
    );
    assert!(
        RESUMED_NOTICE.contains("`gglib remote disable`"),
        "{RESUMED_NOTICE}"
    );
    assert!(
        !RESUMED_NOTICE.contains("now requires"),
        "the key notice belongs to the enable that armed the session: {RESUMED_NOTICE}"
    );
}
