#![doc = include_str!("README.md")]
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
