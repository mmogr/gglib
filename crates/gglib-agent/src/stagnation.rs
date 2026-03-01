//! Text stagnation detection for the agentic loop.
//!
//! # Algorithm
//!
//! After each LLM response, the assistant's text content is hashed with
//! [`crate::fnv1a::fnv1a_64`].  If the hash matches the previous
//! iteration, a stagnation counter is incremented.  When the counter reaches
//! `max_stagnation_steps`, the loop is aborted with
//! [`AgentError::StagnationDetected`], which carries the repeated-text hash,
//! the consecutive count, and the configured limit as structured fields.
//! When the hash changes, the counter is reset to zero.
//!
//! Stagnation detection is a safety net for models that get stuck in a
//! repetitive non-tool-calling loop — e.g., repeatedly summarising their
//! previous answer without making progress.  Tool-call loops are handled
//! separately by [`crate::loop_detection::LoopDetector`].
//!
//! ## Known limitation — oscillation patterns
//!
//! The detector only fires when the **same** hash is repeated **consecutively**.
//! A model oscillating between two distinct responses (A → B → A → B …) resets
//! the counter on every turn and therefore bypasses this guard entirely.
//! Oscillation detection would require a short sliding-window history of past
//! hashes, which is not currently implemented.

use gglib_core::ports::AgentError;

use crate::fnv1a::fnv1a_64;

// =============================================================================
// StagnationDetector
// =============================================================================

/// Stateful guard that detects when the assistant repeats the same text.
///
/// Create once per agent run and call [`StagnationDetector::record`] after
/// every iteration that produces text content.
#[derive(Debug, Default)]
pub(crate) struct StagnationDetector {
    prev_hash: Option<u64>,
    count: usize,
}

impl StagnationDetector {
    /// Record the current assistant text and error if the model has stagnated.
    ///
    /// # Semantics
    ///
    /// The **first** occurrence of any text sets the baseline (no error is
    /// raised).  Subsequent occurrences of the *same* text (same FNV-1a hash)
    /// increment an internal repeat counter starting from 1.  An error is
    /// raised when `repeat_count >= max_steps`, i.e. after the model has
    /// produced the same text `max_steps` times **after** the first baseline
    /// occurrence — meaning the *total* number of identical consecutive
    /// responses before aborting is `max_steps + 1`.
    ///
    /// A different text hash resets the counter to zero.
    ///
    /// | `max_steps` | Identical responses before abort |
    /// |-------------|----------------------------------|
    /// | 0           | 1 (fires on the 1st repeat)       |
    /// | 1           | 2 (fires on the 2nd repeat)       |
    /// | 5 (default) | 6 (fires on the 6th occurrence)   |
    ///
    /// If `text` is empty, the call is a no-op and `Ok(())` is returned
    /// immediately.  Empty responses (tool-call-only iterations) always hash
    /// to the same FNV-1a offset basis value; ignoring them avoids spurious
    /// stagnation detection while the model makes genuine progress through
    /// distinct tool calls.  The caller is therefore not required to guard
    /// against empty strings — this method owns the invariant.
    pub(crate) fn record(&mut self, text: &str, max_steps: usize) -> Result<(), AgentError> {
        if text.is_empty() {
            return Ok(());
        }
        let hash = fnv1a_64(text);
        match self.prev_hash {
            Some(prev) if prev == hash => {
                self.count += 1;
                if self.count >= max_steps {
                    // self.count is the number of *repeats after the first baseline
                    // occurrence*, so total identical responses seen = self.count + 1.
                    return Err(AgentError::StagnationDetected {
                        repeated_text_hash: hash,
                        count: self.count + 1,
                        max_steps,
                    });
                }
            }
            _ => {
                self.count = 0;
                self.prev_hash = Some(hash);
            }
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
        // First occurrence — no stagnation (sets the baseline)
        assert!(det.record(text, 3).is_ok());
        // Second occurrence — count = 1 (< 3)
        assert!(det.record(text, 3).is_ok());
        // Third occurrence — count = 2 (< 3)
        assert!(det.record(text, 3).is_ok());
        // Fourth occurrence — count = 3 (>= 3) → error
        let err = det.record(text, 3).unwrap_err();
        assert!(
            matches!(err, AgentError::StagnationDetected { .. }),
            "expected AgentError::StagnationDetected, got {err:?}"
        );
    }

    #[test]
    fn stagnation_resets_on_new_response() {
        let mut det = StagnationDetector::default();
        let a = "first response";
        let b = "second response";
        assert!(det.record(a, 2).is_ok());
        assert!(det.record(a, 2).is_ok()); // count = 1
        // A different response resets the counter
        assert!(det.record(b, 2).is_ok());
        // Now repeating `b` twice should be fine (count starts at 0 again)
        assert!(det.record(b, 2).is_ok()); // count = 1 (< 2)
    }

    #[test]
    fn stagnation_error_message_contains_count_and_limit() {
        let mut det = StagnationDetector::default();
        let text = "stuck";
        // Trigger stagnation at limit = 1
        assert!(det.record(text, 1).is_ok()); // baseline
        let err = det.record(text, 1).unwrap_err(); // count = 1 >= 1 → error
        if let AgentError::StagnationDetected { count, max_steps, .. } = err {
            assert_eq!(count, 2, "total count should be 2 (baseline + 1 repeat)");
            assert_eq!(max_steps, 1);
        } else {
            panic!("expected AgentError::StagnationDetected");
        }
    }

    #[test]
    fn max_steps_zero_triggers_on_first_repeat() {
        // max_stagnation_steps = 0 means no tolerance at all: the very first
        // repeated response must trigger the error.
        let mut det = StagnationDetector::default();
        let text = "anything";
        // First occurrence sets the baseline (no stagnation yet).
        assert!(
            det.record(text, 0).is_ok(),
            "first occurrence must not error"
        );
        // Second occurrence — count becomes 1, which satisfies count >= 0.
        let err = det.record(text, 0).unwrap_err();
        assert!(
            matches!(err, AgentError::StagnationDetected { .. }),
            "expected StagnationDetected on first repeat with max_steps=0, got {err:?}"
        );
    }
}
