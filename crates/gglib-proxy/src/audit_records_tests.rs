//! Tests for [`super`] — the two records no wire observation corroborates.

use super::*;
use gglib_core::domain::TemplateCaps;

// ── The tri-state, and the collapse it exists to prevent ──────────────────

/// **The rule ADR 0007 decision 3 turns on.** A caps object that did not carry
/// the field must answer "nobody knows", never "the template does not read it"
/// — the second is what the suppression gate acts on, and acting on an absence
/// would delete a level nobody established was ignored.
#[test]
fn an_absent_caps_field_is_not_yet_observed_never_not_supported() {
    let state = EffortSupportState::of(&TemplateCapsState::Read {
        caps: TemplateCaps::default(),
    });

    assert!(
        matches!(state, EffortSupportState::NotYetObserved { .. }),
        "an absent supports_reasoning_effort read as {state:?}"
    );
    assert_ne!(state, EffortSupportState::NotSupported);
}

/// Each of the three causes carries its own reason, so a surface never has to
/// guess which "not observed" it is showing.
#[test]
fn every_unobserved_cause_names_itself() {
    let not_read = EffortSupportState::of(&TemplateCapsState::NotYetRead);
    let unreadable = EffortSupportState::of(&TemplateCapsState::Unreadable {
        reason: "connection refused".to_string(),
    });

    let EffortSupportState::NotYetObserved { reason: first } = not_read else {
        panic!("a state nobody has read is not observed");
    };
    let EffortSupportState::NotYetObserved { reason: second } = unreadable else {
        panic!("a read that failed is not an observation");
    };

    assert!(first.contains("/props"), "{first}");
    assert!(second.contains("connection refused"), "{second}");
    assert_ne!(first, second, "two causes must not share one sentence");
}

/// The two positive answers, which are the only two a gate may act on.
#[test]
fn a_positive_caps_report_answers_yes_or_no() {
    let yes = EffortSupportState::of(&TemplateCapsState::Read {
        caps: TemplateCaps {
            supports_reasoning_effort: Some(true),
            ..TemplateCaps::default()
        },
    });
    let no = EffortSupportState::of(&TemplateCapsState::Read {
        caps: TemplateCaps {
            supports_reasoning_effort: Some(false),
            ..TemplateCaps::default()
        },
    });

    assert_eq!(yes, EffortSupportState::Supported);
    assert_eq!(no, EffortSupportState::NotSupported);
}

/// The three states serialize as three distinct tags. Pinned because every
/// surface branches on this string, and a rename that merged two of them would
/// otherwise be caught only by a human reading a dashboard.
#[test]
fn the_three_states_serialize_distinctly() {
    let tag = |s: &EffortSupportState| {
        serde_json::to_value(s).expect("serializes")["state"]
            .as_str()
            .expect("a tag")
            .to_string()
    };

    assert_eq!(tag(&EffortSupportState::Supported), "supported");
    assert_eq!(tag(&EffortSupportState::NotSupported), "not_supported");
    assert_eq!(
        tag(&EffortSupportState::NotYetObserved {
            reason: String::new()
        }),
        "not_yet_observed"
    );
}

// ── The discarded-name tally ──────────────────────────────────────────────

fn rejected(field: &'static str) -> FieldIssue {
    FieldIssue::Rejected {
        field,
        value: "banana".to_string(),
        expected: "a number",
    }
}

/// **The point of the whole record.** The count said four fields were dropped;
/// only the names can say which, and "why did my reasoning_effort do nothing?"
/// is answerable from one and not the other.
#[test]
fn names_are_kept_beside_the_counts_and_the_two_kinds_stay_apart() {
    let tally = ClientFieldNameTally::default();

    tally.record(
        &["temperature".to_string(), "reasoning_effort".to_string()],
        &[rejected("top_k")],
    );
    tally.record(&["temperature".to_string()], &[]);

    let snap = tally.snapshot();
    let find = |name: &str| {
        snap.fields
            .iter()
            .find(|t| t.field == name)
            .unwrap_or_else(|| panic!("{name} was not tracked: {:?}", snap.fields))
    };

    assert_eq!(find("temperature").discarded, 2);
    assert_eq!(find("temperature").rejected, 0);
    assert_eq!(find("reasoning_effort").discarded, 1);
    assert_eq!(find("top_k").rejected, 1);
    assert_eq!(
        find("top_k").discarded,
        0,
        "a field the gate never saw must not be reported as gated"
    );
    assert_eq!(snap.untracked, 0);
}

/// Most-dropped first, so the field a reader is arguing with is at the top
/// rather than wherever insertion order left it.
#[test]
fn the_tally_is_ordered_by_how_often_each_name_was_dropped() {
    let tally = ClientFieldNameTally::default();
    tally.record(&["top_p".to_string()], &[]);
    for _ in 0..3 {
        tally.record(&["temperature".to_string()], &[]);
    }

    let snap = tally.snapshot();
    assert_eq!(snap.fields.first().expect("a row").field, "temperature");
}

/// **The bound is real and it says so.** An unbounded map keyed by field names
/// on the request path is one refactor away from being client-controlled; a
/// bound that drops silently makes `fields` a claim it cannot support.
#[test]
fn the_table_stops_at_its_bound_and_counts_what_it_could_not_track() {
    let tally = ClientFieldNameTally::default();

    let names: Vec<String> = (0..MAX_TRACKED_FIELD_NAMES + 5)
        .map(|i| format!("field_{i}"))
        .collect();
    tally.record(&names, &[]);

    let snap = tally.snapshot();
    assert_eq!(snap.fields.len(), MAX_TRACKED_FIELD_NAMES);
    assert_eq!(
        snap.untracked, 5,
        "the overflow must be visible, not silent"
    );

    // A name already in the table still counts after the bound is hit —
    // otherwise the bound would freeze the tally rather than cap its width.
    tally.record(&["field_0".to_string()], &[]);
    let snap = tally.snapshot();
    let first = snap
        .fields
        .iter()
        .find(|t| t.field == "field_0")
        .expect("field_0 is tracked");
    assert_eq!(first.discarded, 2);
    assert_eq!(snap.untracked, 5);
}

/// A request that dropped nothing must not touch the lock or the table.
#[test]
fn a_request_with_nothing_dropped_records_nothing() {
    let tally = ClientFieldNameTally::default();
    tally.record(&[], &[]);

    assert_eq!(tally.snapshot(), ClientFieldNames::default());
}

// ── The blindness itself ──────────────────────────────────────────────────

/// The one sentence every surface prints, kept in one place. Its content is
/// the measurement — a reader who loses the citation loses the reason the
/// readback below it is not an observation.
#[test]
fn the_blind_reason_cites_the_measurement_that_established_it() {
    assert!(WIRE_BLIND_REASON.contains("task_params::to_json"));
    assert!(WIRE_BLIND_REASON.contains("ADR 0007 finding 7a"));
}
