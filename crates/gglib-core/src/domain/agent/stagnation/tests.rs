use super::*;

#[test]
fn no_stagnation_for_varied_responses() {
    let mut det = StagnationDetector::default();
    for i in 0..20 {
        assert!(
            det.record(&format!("response {i}"), false, 5).is_ok(),
            "unique responses should never stagnate"
        );
    }
}

#[test]
fn stagnation_triggers_at_limit() {
    let mut det = StagnationDetector::default();
    let text = "I cannot proceed further.";
    // Occurrences 1–3 are within the limit (count ≤ 3).
    assert!(det.record(text, false, 3).is_ok()); // count = 1
    assert!(det.record(text, false, 3).is_ok()); // count = 2
    assert!(det.record(text, false, 3).is_ok()); // count = 3
    // Fourth occurrence — count = 4 (> 3) → error
    let err = det.record(text, false, 3).unwrap_err();
    assert!(
        matches!(err, AgentError::StagnationDetected { .. }),
        "expected AgentError::StagnationDetected, got {err:?}"
    );
}

#[test]
fn different_responses_accumulate_independently() {
    // Each hash has its own counter; B does not affect A's count.
    let mut det = StagnationDetector::default();
    let a = "first response";
    let b = "second response";
    assert!(det.record(a, false, 2).is_ok()); // A×1 (baseline)
    assert!(det.record(a, false, 2).is_ok()); // A×2, prior=1, 1>=2? No
    assert!(det.record(b, false, 2).is_ok()); // B×1 (baseline)
    assert!(det.record(b, false, 2).is_ok()); // B×2, prior=1, 1>=2? No
    // A×3: prior=2, 2>0 && 2>=2 → fire
    let err = det.record(a, false, 2).unwrap_err();
    assert!(
        matches!(err, AgentError::StagnationDetected { count: 3, .. }),
        "expected StagnationDetected with count=3, got {err:?}"
    );
}

#[test]
fn oscillation_abab_fires_stagnation() {
    // A → B → A → B oscillation fires once either hash reaches max_steps+1
    // total occurrences, even though no two consecutive responses match.
    let mut det = StagnationDetector::default();
    let a = "response A";
    let b = "response B";
    assert!(det.record(a, false, 2).is_ok()); // A×1 baseline
    assert!(det.record(b, false, 2).is_ok()); // B×1 baseline
    assert!(det.record(a, false, 2).is_ok()); // A×2, prior=1 < 2
    assert!(det.record(b, false, 2).is_ok()); // B×2, prior=1 < 2
    let err = det.record(a, false, 2).unwrap_err(); // A×3, prior=2 >= 2 → fire
    assert!(
        matches!(err, AgentError::StagnationDetected { count: 3, .. }),
        "expected StagnationDetected with count=3, got {err:?}"
    );
}

#[test]
fn stagnation_error_message_contains_count_and_limit() {
    let mut det = StagnationDetector::default();
    let text = "stuck";
    // With max_steps=1: first occurrence is count=1 (≤ 1, ok); second is count=2 (> 1, error).
    assert!(det.record(text, false, 1).is_ok()); // count = 1
    let err = det.record(text, false, 1).unwrap_err(); // count = 2 > 1 → error
    if let AgentError::StagnationDetected {
        count, max_steps, ..
    } = err
    {
        assert_eq!(
            count, 2,
            "count should be 2 on the first repeat with max_steps=1"
        );
        assert_eq!(max_steps, 1);
    } else {
        panic!("expected AgentError::StagnationDetected");
    }
}

#[test]
fn max_steps_zero_triggers_on_first_occurrence() {
    // max_stagnation_steps = 0 means zero tolerance: count=1 immediately
    // exceeds max_steps=0, so the very first occurrence triggers the error.
    let mut det = StagnationDetector::default();
    let text = "anything";
    let err = det
        .record(text, false, 0)
        .expect_err("max_steps=0 must reject the very first occurrence");
    assert!(
        matches!(
            err,
            AgentError::StagnationDetected {
                count: 1,
                max_steps: 0,
                ..
            }
        ),
        "expected StagnationDetected with count=1 and max_steps=0, got {err:?}"
    );
}

/// The defect this detector had, at its root.
///
/// A small model narrates almost every tool call, and narrates repetitively.
/// Counting that text meant a coding agent doing six different things was
/// refused for saying the same sentence six times.
#[test]
fn a_turn_that_called_a_tool_is_never_recorded() {
    let mut det = StagnationDetector::default();
    for i in 0..50 {
        assert!(
            det.record("Let me look at the file.", true, 5).is_ok(),
            "narration on turn {i} must not count"
        );
    }
    // And it left no trace: the same text as prose still gets its full budget.
    for i in 0..5 {
        assert!(
            det.record("Let me look at the file.", false, 5).is_ok(),
            "prose occurrence {i} is within the threshold"
        );
    }
    assert!(
        det.record("Let me look at the file.", false, 5).is_err(),
        "the sixth prose occurrence is stagnation"
    );
}

/// Six occurrences that would trip if the count never forgot, spread wider than
/// the window. This is what stops a long conversation dying of six scattered
/// "Sure!"s — and, because the proxy replays the whole transcript, dying
/// permanently.
#[test]
fn the_window_forgets_a_repeat_that_has_aged_out() {
    let mut det = StagnationDetector::default();
    for i in 0..6 {
        assert!(det.record("Sure!", false, 5).is_ok(), "occurrence {i}");
        for j in 0..20 {
            assert!(det.record(&format!("distinct {i}-{j}"), false, 5).is_ok());
        }
    }
}

/// Why the window is derived from the threshold rather than fixed.
///
/// At the configured ceiling of 100 a fixed 20-turn window could never hold
/// enough occurrences to trip, which would have disabled the guard outright for
/// anyone who raised the threshold.
#[test]
fn oscillation_still_trips_at_the_ceiling_threshold() {
    let mut det = StagnationDetector::default();
    let fired = (0..1000).any(|i| {
        let text = if i % 2 == 0 { "plan A" } else { "plan B" };
        det.record(text, false, 100).is_err()
    });
    assert!(fired, "oscillation must still trip when max_steps is 100");
}

/// `max_steps = 0` fires on the first occurrence, as it did when the tally was
/// unbounded. The window's `max(1)` is what preserves it.
#[test]
fn a_zero_threshold_fires_on_the_first_prose_turn() {
    let mut det = StagnationDetector::default();
    assert!(det.record("anything", false, 0).is_err());
}
