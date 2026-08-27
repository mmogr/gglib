//! Tests for the half of the verdict that reads what came back.
//!
//! Separate from `tests.rs` because they are about a different question. Those
//! ask whether this is the same batch; these ask whether the same batch got the
//! same answer, which is what decides whether a repeat is a strike.

use serde_json::json;

use super::*;
use crate::ports::AgentError;

// ---- The verdict reads the answers ------------------------------------------

/// One call, so a batch is a batch.
fn one(name: &str) -> Vec<ToolCall> {
    vec![ToolCall {
        id: "c1".into(),
        name: name.into(),
        arguments: json!({}),
    }]
}

/// `check` then `record_results`, in the order both call sites use them.
fn turn(
    det: &mut LoopDetector,
    calls: &[ToolCall],
    answers: Option<u64>,
) -> Result<(), AgentError> {
    let record = det.check(calls, 2, &[], Some(15))?;
    det.record_results(record, answers);
    Ok(())
}

/// The bug this exists to fix: an agent polling a build for output issues an
/// identical batch every time, and run-length counting alone refused it on the
/// third.
#[test]
fn a_repeat_whose_answer_changed_starts_the_run_over() {
    let mut det = LoopDetector::default();
    let calls = one("get_output");
    for i in 0..10u64 {
        assert!(
            turn(&mut det, &calls, Some(i)).is_ok(),
            "poll {i} got a new answer and must not be a strike"
        );
    }
}

/// Kills the inversion. If `record_results` compared an answer against itself —
/// or against the wrong occurrence's — alternating answers would look constant
/// and this would trip on the third turn.
#[test]
fn a_run_whose_answer_alternates_never_trips() {
    let mut det = LoopDetector::default();
    let calls = one("get_output");
    for i in 0..12u64 {
        let answer = Some(i % 2);
        assert!(turn(&mut det, &calls, answer).is_ok(), "turn {i}");
    }
}

/// The baseline survives: same call, same answer, is what the guard is for.
#[test]
fn a_repeat_whose_answer_is_identical_still_trips() {
    let mut det = LoopDetector::default();
    let calls = one("write_file");
    assert!(turn(&mut det, &calls, Some(7)).is_ok());
    assert!(turn(&mut det, &calls, Some(7)).is_ok());
    assert!(turn(&mut det, &calls, Some(7)).is_err(), "third must trip");
}

/// A detector that is never told anything behaves exactly as it did before it
/// could be told. This is what lets every test above this section stand
/// unchanged, and what makes the plumbing commit provably inert.
#[test]
fn an_unrecorded_repeat_trips_exactly_as_before() {
    let mut det = LoopDetector::default();
    let calls = one("write_file");
    assert!(det.check(&calls, 2, &[], Some(15)).is_ok());
    assert!(det.check(&calls, 2, &[], Some(15)).is_ok());
    assert!(det.check(&calls, 2, &[], Some(15)).is_err());
}

/// An answer nobody could read is not evidence of progress. Without this, a
/// client that omits `id` on replayed calls would switch the guard off.
#[test]
fn an_unjoinable_answer_does_not_rescue() {
    let mut det = LoopDetector::default();
    let calls = one("write_file");
    assert!(turn(&mut det, &calls, None).is_ok());
    assert!(turn(&mut det, &calls, None).is_ok());
    assert!(turn(&mut det, &calls, None).is_err(), "third must trip");
}

/// The accepted cost of the reset, at its real width: a run that changed its
/// answer once early takes one more occurrence to trip than it used to.
#[test]
fn a_changed_answer_rescues_only_the_occurrence_that_changed() {
    let mut det = LoopDetector::default();
    let calls = one("write_file");
    assert!(turn(&mut det, &calls, Some(1)).is_ok(), "1st");
    assert!(
        turn(&mut det, &calls, Some(2)).is_ok(),
        "2nd, answer changed"
    );
    assert!(turn(&mut det, &calls, Some(2)).is_ok(), "3rd, answer stood");
    assert!(turn(&mut det, &calls, Some(2)).is_err(), "4th must trip");
}

/// The ceiling. A batch that changes something cannot be carried forever by an
/// output that carries a clock — it gets the read-only allowance and no more.
#[test]
fn a_mutating_run_rescued_by_new_answers_stops_at_the_read_only_allowance() {
    let mut det = LoopDetector::default();
    let calls = one("write_file");
    for i in 0..15u64 {
        assert!(
            turn(&mut det, &calls, Some(i)).is_ok(),
            "occurrence {}",
            i + 1
        );
    }
    assert!(
        turn(&mut det, &calls, Some(99)).is_err(),
        "the 16th must trip however new its answer"
    );
}

/// And the tier that has no ceiling, because repeating a call that changes
/// nothing is free — which is the ground the tier stands on.
#[test]
fn an_observation_run_rescued_by_new_answers_has_no_ceiling() {
    let mut det = LoopDetector::default();
    let calls = one("read_file");
    let patterns = vec!["read_file".to_owned()];
    for i in 0..100u64 {
        let record = det
            .check(&calls, 2, &patterns, Some(15))
            .unwrap_or_else(|e| panic!("occurrence {} must pass: {e}", i + 1));
        det.record_results(record, Some(i));
    }
}

/// With no observation tier configured there is no allowance to lend, so the
/// ceiling collapses onto the strike count and nothing is rescued. Behaviour is
/// exactly what it was before the verdict could read anything.
#[test]
fn a_rescue_ceiling_of_none_disables_the_rescue() {
    let mut det = LoopDetector::default();
    let calls = one("write_file");
    assert!(det.check(&calls, 2, &[], None).is_ok());
    let record = det.check(&calls, 2, &[], None).expect("2nd");
    det.record_results(record, Some(1));
    assert!(
        det.check(&calls, 2, &[], None).is_err(),
        "no observation tier means no rescue, however the answers moved"
    );
}

/// The reset arm has to clear the answers along with the count. Keeping the old
/// run's answer would let the *breaking* batch's first occurrence compare
/// against a stranger, and rescue or condemn itself on that.
#[test]
fn the_batch_that_breaks_a_run_clears_the_answers_it_recorded() {
    let mut det = LoopDetector::default();
    let (a, b) = (one("a"), one("b"));
    assert!(turn(&mut det, &a, Some(1)).is_ok());
    // `b` starts its own run. Its first occurrence has nothing to compare
    // against, so it is not comparable — not a match with a's answer.
    let record = det.check(&b, 2, &[], Some(15)).expect("b: occurrence 1");
    assert_eq!(
        det.record_results(record, Some(1)),
        RepeatOutcome::NotComparable,
        "a fresh run compares against nothing, not against the run it replaced"
    );
}

/// The tier is selected by `observation_tools`, but *waiving the ceiling* is
/// selected by `max_observation_steps`. Reading only the first left a
/// classified batch with no bound at all when the tier was off, so a moving
/// answer carried it forever — the exact opposite of what the field's own doc
/// promised, and unreachable by the mutating case below it.
#[test]
fn a_rescue_ceiling_of_none_disables_the_rescue_for_read_only_batches_too() {
    let mut det = LoopDetector::default();
    let calls = one("read_file");
    let patterns = vec!["read_file".to_owned()];
    let turn = |det: &mut LoopDetector, answer: u64| {
        det.check(&calls, 2, &patterns, None).map(|record| {
            det.record_results(record, Some(answer));
        })
    };
    assert!(turn(&mut det, 1).is_ok(), "1st");
    assert!(turn(&mut det, 2).is_ok(), "2nd, answer changed");
    assert!(
        turn(&mut det, 3).is_err(),
        "with no tier configured a read-only batch has no allowance to be lent"
    );
}

/// The allowance may not tighten the guard for tools it does not classify.
/// `total >= count` always, so a ceiling below `max_strikes` would become the
/// strike threshold and refuse a mutating batch earlier than configured — a
/// user lowering the read-only allowance would silently tighten everything.
#[test]
fn a_low_read_only_allowance_never_tightens_the_mutating_threshold() {
    for allowance in [0usize, 1] {
        let mut det = LoopDetector::default();
        let calls = one("write_file");
        assert!(
            turn(&mut det, &calls, Some(1)).is_ok(),
            "allowance {allowance}: 1st"
        );
        let record = det
            .check(&calls, 2, &[], Some(allowance))
            .unwrap_or_else(|e| panic!("allowance {allowance}: 2nd must pass: {e}"));
        det.record_results(record, Some(1));
        assert!(
            det.check(&calls, 2, &[], Some(allowance)).is_err(),
            "allowance {allowance}: 3rd trips on the strike threshold, as configured"
        );
    }
}

/// Defensive: a record whose run has already been replaced records nothing.
#[test]
fn recording_for_a_batch_that_is_no_longer_the_run_changes_nothing() {
    let mut det = LoopDetector::default();
    let (a, b) = (one("a"), one("b"));
    let stale = det.check(&a, 2, &[], Some(15)).expect("a");
    assert!(
        det.check(&b, 2, &[], Some(15)).is_ok(),
        "b replaces a's run"
    );
    assert_eq!(
        det.record_results(stale, Some(1)),
        RepeatOutcome::NotTheCurrentRun
    );
}

/// The outcomes the proxy's ledger reads.
#[test]
fn the_outcome_names_what_happened() {
    let mut det = LoopDetector::default();
    let calls = one("write_file");
    let first = det.check(&calls, 2, &[], Some(15)).expect("1st");
    assert_eq!(
        det.record_results(first, Some(1)),
        RepeatOutcome::NotComparable,
        "nothing to compare against yet"
    );
    let second = det.check(&calls, 2, &[], Some(15)).expect("2nd");
    assert_eq!(
        det.record_results(second, Some(2)),
        RepeatOutcome::AnswerChanged
    );
    let third = det
        .check(&calls, 2, &[], Some(15))
        .expect("rescued by the 2nd");
    assert_eq!(
        det.record_results(third, Some(2)),
        RepeatOutcome::AnswerRepeated
    );
}

/// A rescue arrives one turn too late to save a run already at its threshold.
///
/// `record_results` runs after `check`, so the occurrence that would have
/// changed the answer never gets to report it — on the agent path its batch is
/// refused before it executes, which is the point. The cost is stated here
/// rather than discovered: a stuck run that was about to produce something new
/// is still refused.
#[test]
fn a_run_at_its_threshold_trips_before_the_next_answer_can_rescue_it() {
    let mut det = LoopDetector::default();
    let calls = one("write_file");
    assert!(turn(&mut det, &calls, Some(1)).is_ok(), "1st");
    assert!(turn(&mut det, &calls, Some(1)).is_ok(), "2nd, answer stood");
    assert!(
        det.check(&calls, 2, &[], Some(15)).is_err(),
        "the 3rd is refused, and never gets to say what it would have returned"
    );
}
