//! Tests for [`super::run`] — the guard's step, with no server around it.
//!
//! These characterise what the step answers with the guard on and off. Each
//! one pins something a caller depends on: that the switch-off path scans
//! nothing, that a first trip is forwarded with a note, and that the readings
//! the dashboard shows are recorded whether or not the verdict trips.

use bytes::Bytes;
use gglib_core::Settings;
use gglib_core::domain::defects::LoopGuardTrip;
use serde_json::json;

use super::fixtures::{
    MODEL, agentic_body, body, counts, looping, observing, repeated_read, stagnating, store,
    switched_off,
};
use super::{GuardStep, run};

#[test]
fn a_guard_switched_off_scans_nothing_and_forwards() {
    let (metrics, ledger) = store();

    let step = run(
        &switched_off(),
        &body(looping(3)),
        MODEL,
        &observing(&metrics),
    );

    assert!(matches!(step, GuardStep::Forward));
    // Nothing was scanned, so nothing was recorded: no snapshot, and not the
    // repeat readings a scan would have produced.
    assert_eq!(metrics.total_requests(), 0);
    let c = counts(&ledger);
    assert_eq!(c.loop_guard_trips, 0);
    // All three diagnostic readings, not one: off means the scan does not
    // run, so none of them can be taken. Asserting a single reading would
    // pass for a guard that scanned and merely declined to act.
    assert_eq!(c.identical_result_repeats, 0);
    assert_eq!(c.repeats_not_evaluated, 0);
    assert_eq!(c.repeats_rescued, 0);
}

#[test]
fn a_benign_history_is_forwarded() {
    let (metrics, ledger) = store();

    let step = run(
        &Settings::with_defaults(),
        &body(vec![json!({ "role": "assistant", "content": "done" })]),
        MODEL,
        &observing(&metrics),
    );

    assert!(matches!(step, GuardStep::Forward));
    assert_eq!(counts(&ledger).loop_guard_trips, 0);
}

#[test]
fn an_unparseable_body_is_forwarded() {
    let (metrics, _ledger) = store();

    let step = run(
        &Settings::with_defaults(),
        &Bytes::from_static(b"not json at all"),
        MODEL,
        &observing(&metrics),
    );

    // Fail-open: this guard is protection, not validation.
    assert!(matches!(step, GuardStep::Forward));
    assert_eq!(metrics.total_requests(), 0);
}

#[test]
fn a_repeat_the_verdict_cannot_see_is_still_read() {
    let (metrics, ledger) = store();

    // Two identical read-only batches with identical results: never a verdict
    // at any count, but the reading the dashboard shows is taken anyway.
    let step = run(
        &Settings::with_defaults(),
        &agentic_body(repeated_read(2)),
        MODEL,
        &observing(&metrics),
    );

    assert!(matches!(step, GuardStep::Forward));
    let c = counts(&ledger);
    assert_eq!(c.loop_guard_trips, 0);
    assert_eq!(
        c.identical_result_repeats, 1,
        "the diagnosis is recorded for every scanned request, trip or not"
    );
    assert_eq!(
        c.repeats_not_evaluated, 0,
        "the results were joinable, so nothing went unevaluated"
    );
}

#[test]
fn a_first_trip_is_noted_and_records_nothing_itself() {
    for (history, expected) in [
        (looping(3), LoopGuardTrip::Loop),
        (stagnating(6), LoopGuardTrip::Stagnation),
    ] {
        let (metrics, ledger) = store();

        let GuardStep::Note { note, trip } = run(
            &Settings::with_defaults(),
            &body(history),
            MODEL,
            &observing(&metrics),
        ) else {
            panic!("a first trip is noted");
        };

        assert_eq!(trip, expected, "the note carries its own detector");
        assert!(
            note.text().contains("[gglib loop guard]"),
            "{}",
            note.text()
        );
        // No snapshot of its own: the request goes on to be forwarded, and
        // the forward records it. Recording here too would count it twice.
        assert_eq!(metrics.total_requests(), 0);
        assert_eq!(counts(&ledger).loop_guard_trips, 0);
    }
}
