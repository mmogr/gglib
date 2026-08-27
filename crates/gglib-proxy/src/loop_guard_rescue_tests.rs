//! What `repeats_rescued` counts, and what it must not.
//!
//! A fourth test module because `loop_guard_tests.rs` is frozen at its current
//! size by the complexity ratchet. Helpers are duplicated for the same reason.

use super::*;
use serde_json::json;

fn cfg() -> LoopGuardConfig {
    LoopGuardConfig::from_settings(&Settings::with_defaults()).expect("guard on by default")
}

fn body(messages: &[Value]) -> Vec<u8> {
    json!({ "model": "m", "messages": messages })
        .to_string()
        .into_bytes()
}

/// `n` occurrences of one mutating batch, each answered differently.
fn moving_writes(n: usize) -> Vec<Value> {
    (0..n)
        .flat_map(|i| {
            [
                json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": format!("w{i}"),
                        "type": "function",
                        "function": { "name": "write_file", "arguments": "{}" }
                    }]
                }),
                json!({ "role": "tool", "tool_call_id": format!("w{i}"), "content": format!("{i} changed") }),
            ]
        })
        .collect()
}

/// The defect. `max_repeated_batch_steps` is 2, so the *second* occurrence of a
/// batch passes `check` whatever its answer was — nothing was withheld. The
/// counter fired there anyway, because it was derived from `record_results`
/// returning `AnswerChanged`, which has no threshold in scope.
///
/// Four surfaces describe this counter as the turn "the loop guard declined to
/// act" on, and ADR 0010's third kill criterion reads it. In a fleet where
/// nothing reaches a third occurrence, every count was a turn that would have
/// passed anyway — so the reading said the arm had customers when it had none.
#[test]
fn a_repeat_that_was_never_at_risk_is_not_a_rescue() {
    let outcome = scan_history(&body(&moving_writes(2)), &cfg());
    assert_eq!(outcome.verdict, LoopGuardVerdict::Pass);
    assert!(
        !outcome.repeat_rescued,
        "the second occurrence is inside the allowance: {outcome:?}"
    );
}

/// The third is. Without result-awareness this turn was an HTTP 400, so it is
/// exactly the turn the counter claims to be counting.
#[test]
fn the_first_occurrence_past_the_threshold_is_a_rescue() {
    let outcome = scan_history(&body(&moving_writes(3)), &cfg());
    assert_eq!(outcome.verdict, LoopGuardVerdict::Pass);
    assert!(outcome.repeat_rescued, "{outcome:?}");
}

/// And every one after it, up to the ceiling. `total > 15` is where the
/// read-only allowance ends, so 15 is the last occurrence that can be rescued.
#[test]
fn every_later_occurrence_is_a_rescue_too() {
    for n in 3..=15 {
        let outcome = scan_history(&body(&moving_writes(n)), &cfg());
        assert_eq!(outcome.verdict, LoopGuardVerdict::Pass, "n = {n}");
        assert!(outcome.repeat_rescued, "n = {n}: {outcome:?}");
    }
}

/// The ceiling still ends it. A rescue is a turn the guard let through, not a
/// turn it cannot reach — past `max_observation_steps` the batch is refused and
/// there is nothing to report.
#[test]
fn the_ceiling_ends_the_rescues_rather_than_extending_them() {
    let outcome = scan_history(&body(&moving_writes(16)), &cfg());
    assert!(
        matches!(outcome.verdict, LoopGuardVerdict::LoopDetected { .. }),
        "{outcome:?}"
    );
    assert!(
        !outcome.repeat_rescued,
        "a refused turn was not let through: {outcome:?}"
    );
}

/// A first occurrence has nothing to compare and is not a rescue.
#[test]
fn a_batch_seen_once_is_not_a_rescue() {
    let outcome = scan_history(&body(&moving_writes(1)), &cfg());
    assert!(!outcome.repeat_rescued, "{outcome:?}");
}
