#![doc = include_str!("README.md")]
#[cfg(test)]
mod tests;

use std::collections::VecDeque;

use super::fnv1a::fnv1a_64;
use crate::ports::AgentError;

// =============================================================================
// StagnationDetector
// =============================================================================

/// How many turns of history the detector weighs, as a multiple of
/// `max_stagnation_steps`.
///
/// Derived rather than a second knob, because the two numbers are not
/// independent: a window shorter than the repeats it takes to trip makes the
/// guard unable to fire at all. At the ceiling of 100 a fixed window of, say,
/// 20 would silently disable it.
///
/// Four, because oscillation is the tightest constraint. Catching A → B → A → B
/// needs `max_steps + 1` occurrences of one text, which take `2 × (max_steps +
/// 1)` turns to arrive; the window has to be at least that long or the pair
/// ages out of it. Four clears that for every `max_steps ≥ 1` with room to
/// spare, and at the default of 5 gives a 20-turn window against the 12 turns
/// oscillation actually needs.
const WINDOW_FACTOR: usize = 4;

/// Stateful guard that detects when the assistant repeats the same text.
///
/// Create once per agent run and call [`StagnationDetector::record`] after
/// every iteration that produces text content.
///
/// Holds a sliding window of recent **prose** turns rather than a session-wide
/// tally, and ignores any turn that called a tool. See the module docs for why
/// each of those is load bearing.
#[derive(Debug, Default)]
pub struct StagnationDetector {
    /// Hashes of the recent prose turns, oldest first, capped at the window.
    ///
    /// A `VecDeque` and a linear count rather than a map: the window is a small
    /// multiple of a threshold whose default is 5, so counting is cheaper than
    /// maintaining a second index — and a map cannot express "forget the oldest
    /// turn", which is the whole point.
    recent: VecDeque<u64>,
}

impl StagnationDetector {
    /// Forget the window, as a user turn does.
    ///
    /// The counterpart to [`LoopDetector::break_run`](crate::domain::agent::LoopDetector::break_run),
    /// and the other half of what a fresh detector gives the agent path. A
    /// window alone cannot rescue a conversation whose repeats were *adjacent*:
    /// they stay in the transcript, and a replayed scan trips on them at the
    /// same point forever. Someone saying something new is what moves on.
    pub fn clear(&mut self) {
        self.recent.clear();
    }

    /// Record one assistant turn and error if the model has stagnated.
    ///
    /// `made_tool_calls` says whether this turn also issued a tool-call batch.
    /// Such a turn is **not recorded at all**: it is doing work, and
    /// [`LoopDetector`](crate::domain::agent::LoopDetector) is what judges that
    /// work. A parameter rather than a decision left to each caller, so the two
    /// paths that run this detector cannot answer it differently.
    ///
    /// Empty text is silently ignored (tool-call-only iterations, which the
    /// line above already covers, and turns that produced nothing).
    ///
    /// An error is raised when the number of occurrences **within the window**
    /// exceeds `max_steps`:
    ///
    /// | `max_steps` | Identical prose turns before abort |
    /// |-------------|------------------------------------|
    /// | 0           | 1 (fires on first occurrence)      |
    /// | 1           | 2 (fires on first repeat)          |
    /// | 5 (default) | 6, within 20 turns                 |
    pub fn record(
        &mut self,
        text: &str,
        made_tool_calls: bool,
        max_steps: usize,
    ) -> Result<(), AgentError> {
        if made_tool_calls || text.is_empty() {
            return Ok(());
        }
        // `max(1)` keeps `max_steps = 0` firing on the first occurrence, which
        // is what it did when the tally was unbounded. A zero-length window
        // would drop the hash before it could be counted.
        let window = max_steps.saturating_mul(WINDOW_FACTOR).max(1);

        let hash = fnv1a_64(text);
        self.recent.push_back(hash);
        while self.recent.len() > window {
            self.recent.pop_front();
        }

        let count = self.recent.iter().filter(|&&h| h == hash).count();
        if count > max_steps {
            return Err(AgentError::StagnationDetected {
                repeated_text_hash: format!("{hash:016x}"),
                count,
                max_steps,
            });
        }
        Ok(())
    }
}
