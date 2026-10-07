//! The iteration and stagnation limits a turn's loop runs with, resolved by
//! the one rule every caller of the agent loop uses: the daemon, for the
//! page's run and a paired device's turn, and the CLI's `chat` and `q`.

use super::config::DEFAULT_MAX_ITERATIONS;
use crate::settings::Settings;

/// The limits one turn's loop runs with, as
/// [`AgentConfig::from_user_params`](super::AgentConfig::from_user_params)
/// takes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnLimits {
    /// The most LLM→tool→LLM iterations.
    pub max_iterations: usize,
    /// How often one reply may repeat before the stagnation guard stops the
    /// loop; `None` keeps the built-in default.
    pub max_stagnation_steps: Option<usize>,
}

impl TurnLimits {
    /// Resolve a turn's limits against this machine's `settings`.
    ///
    /// The iteration limit is the one the turn `named` (its request's, or
    /// failing that its chat's), then the stored `max_tool_iterations`,
    /// then [`DEFAULT_MAX_ITERATIONS`]. The stagnation limit is the stored
    /// `max_stagnation_steps` alone: no request and no chat names one.
    /// `settings` is `None` when they could not be read, which leaves the
    /// built-in defaults.
    #[must_use]
    pub fn resolve(named: Option<usize>, settings: Option<&Settings>) -> Self {
        let stored = |limit: Option<u32>| limit.map(|n| n as usize);
        Self {
            max_iterations: named
                .or_else(|| stored(settings.and_then(|s| s.max_tool_iterations)))
                .unwrap_or(DEFAULT_MAX_ITERATIONS),
            max_stagnation_steps: stored(settings.and_then(|s| s.max_stagnation_steps)),
        }
    }
}

#[cfg(test)]
#[path = "limits_tests.rs"]
mod limits_tests;
