//! Text stagnation detection for the agentic loop.
//!
//! # Algorithm
//!
//! After each LLM response the assistant's text is hashed with
//! [`crate::fnv1a::fnv1a_64`].  The hash is looked up in a session-wide
//! occurrence map.  When a hash has already been seen at least
//! `max_stagnation_steps` times **before** the current call, the loop is
//! aborted with [`AgentError::StagnationDetected`].
//!
//! Stagnation detection is a safety net for models that get stuck in a
//! repetitive non-tool-calling loop.  Tool-call loops are handled separately
//! by [`crate::loop_detection::LoopDetector`].
//!
//! ## Oscillation detection
//!
//! Because occurrence counts are accumulated across the **whole session**, the
//! detector catches A → B → A → B oscillations as well as strictly consecutive
//! repetitions.  A model that alternates between two responses will exhaust its
//! budget for each response independently; with the default `max_stagnation_steps
//! = 5`, stagnation fires within at most 12 iterations (two responses × 6
//! occurrences each).
//!
//! The first occurrence of any hash is always treated as a baseline and never
//! triggers an error.  Empty text is silently ignored so that tool-call-only
//! iterations do not accumulate spurious counts.

use std::collections::HashMap;

use gglib_core::ports::AgentError;

use crate::fnv1a::fnv1a_64;

// =============================================================================
// StagnationDetector
// =============================================================================

/// Stateful guard that detects when the assistant repeats the same text.
///
/// Create once per agent run and call [`StagnationDetector::record`] after
/// every iteration that produces text content.
///
/// Tracks **session-wide** occurrence counts per hash so that both strictly
/// consecutive repetitions and A → B → A oscillations are caught.
#[derive(Debug, Default)]
pub struct StagnationDetector {
    occurrences: HashMap<u64, usize>,
}

impl StagnationDetector {
    /// Record the current assistant text and error if the model has stagnated.
    ///
    /// Each call increments the session-wide occurrence counter for the text
    /// hash.  An error is raised when the counter **after** incrementing
    /// exceeds `max_steps`:
    ///
    /// | `max_steps` | Total identical responses before abort |
    /// |-------------|----------------------------------------|
    /// | 0           | 1 (fires on first occurrence)          |
    /// | 1           | 2 (fires on first repeat)              |
    /// | 5 (default) | 6 (fires on sixth occurrence)          |
    ///
    /// Empty text is silently ignored (tool-call-only iterations).
    pub(crate) fn record(&mut self, text: &str, max_steps: usize) -> Result<(), AgentError> {
        if text.is_empty() {
            return Ok(());
        }
        let hash = fnv1a_64(text);
        let count = self.occurrences.entry(hash).or_insert(0);
        *count += 1;
        if *count > max_steps {
            return Err(AgentError::StagnationDetected {
                repeated_text_hash: format!("{hash:016x}"),
                count: *count,
                max_steps,
            });
        }
        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_stagnation_for_varied_responses() {
        let mut det = StagnationDetector::default();
        for i in 0..20 {
            assert!(
                det.record(&format!("response {i}"), 5).is_ok(),
                "unique responses should never stagnate"
            );
        }
    }

    #[test]
    fn stagnation_triggers_at_limit() {
        let mut det = StagnationDetector::default();
        let text = "I cannot proceed further.";
        // Occurrences 1–3 are within the limit (count ≤ 3).
        assert!(det.record(text, 3).is_ok()); // count = 1
        assert!(det.record(text, 3).is_ok()); // count = 2
        assert!(det.record(text, 3).is_ok()); // count = 3
        // Fourth occurrence — count = 4 (> 3) → error
        let err = det.record(text, 3).unwrap_err();
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
        assert!(det.record(a, 2).is_ok()); // A×1 (baseline)
        assert!(det.record(a, 2).is_ok()); // A×2, prior=1, 1>=2? No
        assert!(det.record(b, 2).is_ok()); // B×1 (baseline)
        assert!(det.record(b, 2).is_ok()); // B×2, prior=1, 1>=2? No
        // A×3: prior=2, 2>0 && 2>=2 → fire
        let err = det.record(a, 2).unwrap_err();
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
        assert!(det.record(a, 2).is_ok()); // A×1 baseline
        assert!(det.record(b, 2).is_ok()); // B×1 baseline
        assert!(det.record(a, 2).is_ok()); // A×2, prior=1 < 2
        assert!(det.record(b, 2).is_ok()); // B×2, prior=1 < 2
        let err = det.record(a, 2).unwrap_err(); // A×3, prior=2 >= 2 → fire
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
        assert!(det.record(text, 1).is_ok()); // count = 1
        let err = det.record(text, 1).unwrap_err(); // count = 2 > 1 → error
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
            .record(text, 0)
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
}
