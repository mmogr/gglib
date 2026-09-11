//! Tests for the copy `gglib remote connect` prints about its own rename.

use super::*;

/// The notice has to name **both** commands, and a rename is the one edit
/// that will quietly take one of them away.
///
/// This is not hypothetical. A sweep that replaced `gglib remote connect`
/// with `gglib remote join` across 28 files rewrote this sentence into
/// "`gglib remote join` is `gglib remote join` now" — printed to every
/// person who typed the old name, and caught by nothing, because the only
/// test near it pinned a different substring. Asserted on the constant
/// rather than on captured stderr because `connect` cannot be reached
/// without a daemon, and the string is the whole of what this guards.
#[test]
fn the_rename_notice_names_the_old_name_and_the_new_one() {
    let notice = RENAME_NOTICE.join(" ");

    assert!(
        notice.contains("`gglib remote connect`"),
        "the name the person typed has to appear, or the note explains nothing: {notice}"
    );
    assert!(
        notice.contains("`gglib remote join`"),
        "and so does the name they should type next: {notice}"
    );
}

/// The reassurance is the other half of the job: someone who has already
/// paired needs to know the rename costs them nothing.
#[test]
fn the_rename_notice_says_an_existing_pairing_is_unaffected() {
    let notice = RENAME_NOTICE.join(" ");

    assert!(
        notice.contains("still works"),
        "the old name is not being withdrawn today: {notice}"
    );
    assert!(
        notice.contains("pairing you already have"),
        "and a stored pairing is untouched: {notice}"
    );
}

/// The house style for a `note:` block: lowercase `note:`, two-space indent,
/// continuation lines aligned under the word after it, and nothing wider
/// than eighty columns.
#[test]
fn the_rename_notice_is_shaped_like_every_other_note() {
    assert!(
        RENAME_NOTICE[0].starts_with("  note: "),
        "first line: {}",
        RENAME_NOTICE[0]
    );
    assert!(
        RENAME_NOTICE[1].starts_with("        "),
        "the continuation aligns under the text, not the marker: {}",
        RENAME_NOTICE[1]
    );
    for line in RENAME_NOTICE {
        assert!(
            line.chars().count() <= 80,
            "a terminal is eighty columns until proven otherwise: {line}"
        );
    }
}
