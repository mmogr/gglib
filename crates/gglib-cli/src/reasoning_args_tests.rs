//! Tests for [`super`] — the two reasoning value parsers and their refusals,
//! and the note for a level the template does not list.

use super::*;

use ReasoningEffort::{Low, Medium, XHigh};

/// Qwen3.8's list: the three levels its template renders.
const QWEN: &[ReasoningEffort] = &[Low, Medium, XHigh];

/// Qwen3.8's list and `max`: the note names the listed levels, the level,
/// and that it is kept.
#[test]
fn an_unlisted_level_earns_a_note_naming_the_listed_levels() {
    let note = unlisted_effort_note(ReasoningEffort::Max, Some(QWEN), "is stored").expect("a note");

    assert_eq!(
        note,
        "Note: this model's template lists the reasoning-effort levels low, medium, xhigh, \
         and not max. The level is stored all the same: one the list leaves out can still work."
    );
}

#[test]
fn a_listed_level_earns_no_note() {
    assert_eq!(unlisted_effort_note(XHigh, Some(QWEN), "is sent"), None);
}

/// An unknown list never earns a note.
#[test]
fn an_unknown_list_earns_no_note() {
    assert_eq!(
        unlisted_effort_note(ReasoningEffort::Max, None, "is sent"),
        None
    );
}

#[test]
fn an_empty_list_notes_every_level() {
    let note = unlisted_effort_note(Low, Some(&[]), "is sent").expect("a note");
    assert!(note.contains("lists no reasoning-effort level"), "{note}");
    assert!(note.contains("The level is sent all the same"), "{note}");
}

#[test]
fn accepts_every_level_in_any_case() {
    for level in ReasoningEffort::ALL {
        assert_eq!(parse_effort(level.as_str()), Ok(level));
        assert_eq!(parse_effort(&level.as_str().to_uppercase()), Ok(level));
    }
}

/// `xhigh` is one word on the wire. A user who types the snake-cased spelling
/// gets the vocabulary back rather than a silent near-miss.
#[test]
fn rejects_the_snake_cased_spelling_of_xhigh() {
    let err = parse_effort("x_high").expect_err("not a level");
    assert!(err.contains("xhigh"), "got: {err}");
}

/// The one refusal that has to teach rather than merely refuse.
#[test]
fn none_is_refused_by_pointing_at_the_budget_flag() {
    for spelling in ["none", "None", "NONE"] {
        let err = parse_effort(spelling).expect_err("'none' is not offered");
        assert!(
            err.contains("--reasoning-budget-tokens 0"),
            "names the flag that actually stops thinking: {err}"
        );
        assert!(
            err.contains("medium"),
            "says what 'none' would really have done: {err}"
        );
    }
}

#[test]
fn an_unknown_level_lists_the_vocabulary() {
    let err = parse_effort("banana").expect_err("not a level");
    assert!(err.contains("minimal"), "got: {err}");
    assert!(err.contains("max"), "got: {err}");
    assert!(
        !err.contains("--reasoning-budget-tokens"),
        "only 'none' earns the pointer: {err}"
    );
}

/// The three values that mean something specific, plus the boundary upstream
/// draws. `-1` and `0` are both valid and mean opposite things.
#[test]
fn accepts_upstreams_whole_range() {
    assert_eq!(parse_budget("-1"), Ok(-1));
    assert_eq!(parse_budget("0"), Ok(0));
    assert_eq!(parse_budget("4096"), Ok(4096));
    assert_eq!(parse_budget(&i32::MAX.to_string()), Ok(i32::MAX));
}

#[test]
fn rejects_below_minus_one_with_upstreams_own_range() {
    for value in ["-2", "-100"] {
        let err = parse_budget(value).expect_err("below upstream's range");
        assert_eq!(err, format!("expected {BUDGET_RANGE}"));
    }
}

/// A non-integer and an out-of-i32 value are the same class of mistake as an
/// out-of-range one, and get the same sentence rather than a parser's.
#[test]
fn unparseable_values_get_the_range_message_too() {
    for value in ["", "1.5", "lots", "2147483648"] {
        let err = parse_budget(value).expect_err("not an i32 in range");
        assert!(
            err.contains("-1 defers to the launch default"),
            "got: {err}"
        );
    }
}
