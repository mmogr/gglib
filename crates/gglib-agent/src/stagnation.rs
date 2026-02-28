//! Text stagnation detection for the agentic loop.
//!
//! This module is a port of stagnation detection from
//! `src/hooks/useGglibRuntime/agentLoop.ts` (`MAX_STAGNATION_STEPS`).
//!
//! # Algorithm
//!
//! After each LLM response, the assistant's text content is hashed with
//! [`crate::loop_detection::fnv1a_32`].  If the hash matches the previous
//! iteration, a stagnation counter is incremented.  When the counter reaches
//! `max_stagnation_steps`, the loop is aborted with an
//! [`AgentError::Internal`] describing the stagnation.  When the hash
//! changes, the counter is reset to zero.
//!
//! Stagnation detection is a safety net for models that get stuck in a
//! repetitive non-tool-calling loop — e.g., repeatedly summarising their
//! previous answer without making progress.  Tool-call loops are handled
//! separately by [`crate::loop_detection::LoopDetector`].

use gglib_core::ports::AgentError;

use crate::loop_detection::fnv1a_32;

// =============================================================================
// StagnationDetector
// =============================================================================

/// Stateful guard that detects when the assistant repeats the same text.
///
/// Create once per agent run and call [`StagnationDetector::record`] after
/// every iteration that produces text content.
#[derive(Debug, Default)]
pub struct StagnationDetector {
    prev_hash: Option<u32>,
    count: usize,
}

impl StagnationDetector {
    /// Create a fresh detector with empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the current assistant text and error if the model has stagnated.
    ///
    /// When `text` hashes to the same value as the previous call,
    /// the stagnation counter is incremented.  When the counter reaches
    /// `max_steps`, [`AgentError::Internal`] is returned.  On a new hash
    /// the counter resets to zero.
    ///
    /// Note: empty text is hashed normally.  A model that consistently produces
    /// no text content (e.g., only tool calls) will have the same hash each
    /// iteration but this is expected and harmless because the tool-call loop
    /// detector handles that case.
    pub fn record(&mut self, text: &str, max_steps: usize) -> Result<(), AgentError> {
        let hash = fnv1a_32(text);
        match self.prev_hash {
            Some(prev) if prev == hash => {
                self.count += 1;
                if self.count >= max_steps {
                    return Err(AgentError::Internal(format!(
                        "agent stagnated: identical response text seen {count} time(s) in a row \
                        (max_stagnation_steps = {max_steps})",
                        count = self.count,
                    )));
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
        let mut det = StagnationDetector::new();
        for i in 0..20 {
            assert!(
                det.record(&format!("response {i}"), 5).is_ok(),
                "unique responses should never stagnate"
            );
        }
    }

    #[test]
    fn stagnation_triggers_at_limit() {
        let mut det = StagnationDetector::new();
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
            matches!(err, AgentError::Internal(_)),
            "expected AgentError::Internal, got {err:?}"
        );
    }

    #[test]
    fn stagnation_resets_on_new_response() {
        let mut det = StagnationDetector::new();
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
        let mut det = StagnationDetector::new();
        let text = "stuck";
        // Trigger stagnation at limit = 1
        assert!(det.record(text, 1).is_ok()); // baseline
        let err = det.record(text, 1).unwrap_err(); // count = 1 >= 1 → error
        if let AgentError::Internal(msg) = err {
            assert!(msg.contains('1'), "message should contain the count: {msg}");
        } else {
            panic!("expected AgentError::Internal");
        }
    }
}
