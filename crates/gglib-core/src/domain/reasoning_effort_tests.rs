//! Tests for [`super::ReasoningEffort`].

use super::ReasoningEffort;

/// **The pin.** `serde(rename_all = "lowercase")` must produce `xhigh`, one
/// word. `snake_case` would produce `x_high`, which llama-server does not
/// recognise and does not reject either — it would render `Reasoning: x_high`
/// into the prompt and look like it worked.
#[test]
fn the_wire_spelling_of_every_level_is_pinned() {
    let spelled: Vec<String> = ReasoningEffort::ALL
        .iter()
        .map(|level| serde_json::to_string(level).expect("a unit variant serialises"))
        .collect();

    assert_eq!(
        spelled,
        ["minimal", "low", "medium", "high", "xhigh", "max"].map(|s| format!("\"{s}\"")),
    );
}

/// `as_str` writes no request body, so nothing would fail if it drifted from
/// what `serde` emits — except a client's level being read as one thing and
/// sent as another.
#[test]
fn as_str_agrees_with_serde() {
    for level in ReasoningEffort::ALL {
        let serialised = serde_json::to_value(level).expect("a unit variant serialises");
        assert_eq!(serialised, serde_json::json!(level.as_str()));
        assert_eq!(level.to_string(), level.as_str());
    }
}

#[test]
fn every_level_round_trips_through_the_wire_spelling() {
    for level in ReasoningEffort::ALL {
        assert_eq!(ReasoningEffort::from_wire(level.as_str()), Some(level));
    }
    assert_eq!(
        ReasoningEffort::from_wire("HIGH"),
        Some(ReasoningEffort::High)
    );
}

/// The measured wire fact this whole type exists for: llama-server renders
/// `"banana"` into the prompt verbatim. gglib does not.
#[test]
fn a_level_that_is_not_a_level_is_not_read_as_one() {
    assert_eq!(ReasoningEffort::from_wire("banana"), None);
    assert_eq!(ReasoningEffort::from_wire(""), None);
    assert_eq!(ReasoningEffort::from_wire("x_high"), None);
}

/// ADR 0007 decision 4. `"none"` is a value llama-server accepts and gglib
/// refuses, because on `gpt-oss` it delivers *medium* thinking — the template's
/// own fallback fires once the kwarg is erased. It must never become a
/// variant, and it must never be read as one either.
#[test]
fn none_is_not_a_level() {
    assert_eq!(ReasoningEffort::from_wire("none"), None);
    assert!(
        !ReasoningEffort::ALL
            .iter()
            .any(|level| level.as_str() == "none")
    );
}

#[test]
fn the_vocabulary_names_every_level_and_nothing_else() {
    assert_eq!(
        ReasoningEffort::wire_vocabulary(),
        "minimal, low, medium, high, xhigh, max"
    );
}
