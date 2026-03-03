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

#[cfg(test)]
mod tests;

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
