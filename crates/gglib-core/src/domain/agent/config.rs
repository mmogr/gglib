//! [`AgentConfig`] — configuration for a single agentic loop run.

use serde::Serialize;

/// Configuration that governs a single agentic loop run.
///
/// All fields have sensible defaults via [`Default`] that match the constants
/// used in the TypeScript frontend (`src/hooks/useGglibRuntime/agentLoop.ts`).
///
/// # Serialisation
///
/// `AgentConfig` is intentionally **not** `Deserialize`.  External callers
/// (HTTP, future config files) must go through a dedicated DTO that exposes
/// only the safe subset of fields.  This prevents accidental exposure of
/// internal tuning knobs (pruning parameters, strike limits, etc.) to
/// untrusted callers.
#[derive(Debug, Clone, Serialize)]
pub struct AgentConfig {
    /// Maximum number of LLM→tool→LLM iterations before the loop is aborted.
    ///
    /// Frontend constant: `DEFAULT_MAX_TOOL_ITERS = 25`.
    pub max_iterations: usize,

    /// Maximum number of tool calls that may be executed in parallel per iteration.
    ///
    /// Frontend constant: `MAX_PARALLEL_TOOLS = 5`.
    pub max_parallel_tools: usize,

    /// Per-tool execution timeout in milliseconds.
    ///
    /// Frontend constant: `TOOL_TIMEOUT_MS = 30_000`.
    pub tool_timeout_ms: u64,

    /// Maximum total character budget across all messages before context pruning
    /// is applied.
    ///
    /// Frontend constant: `MAX_CONTEXT_CHARS = 180_000`.
    pub context_budget_chars: usize,

    /// Maximum number of times the same tool-call batch signature may repeat
    /// consecutively before the loop is declared stuck and aborted with
    /// [`crate::ports::AgentError::LoopDetected`].
    ///
    /// Frontend constant: `MAX_SAME_SIGNATURE_HITS = 2` in `agentLoop.ts`.
    ///
    /// Set to `None` to disable loop detection entirely (useful in tests that
    /// deliberately repeat the same tool call).
    pub max_protocol_strikes: Option<usize>,

    /// Number of consecutive iterations in which the assistant produces identical
    /// text content before the loop is considered stagnant and aborted.
    ///
    /// Frontend constant: `MAX_STAGNATION_STEPS = 5`.
    ///
    /// Set to `None` to disable stagnation detection entirely (useful in tests
    /// that return a fixed LLM response across many iterations).
    pub max_stagnation_steps: Option<usize>,

    /// Number of most-recent tool-result messages preserved during the first
    /// pass of context pruning.
    ///
    /// Not exposed as a user-facing option because the value is calibrated
    /// to balance context retention against token budget; changing it
    /// independently of `context_budget_chars` can produce incoherent
    /// conversation histories.
    pub prune_keep_tool_messages: usize,

    /// Number of non-system messages retained during the emergency tail-prune
    /// pass (second pass of context pruning).
    ///
    /// Same rationale as [`Self::prune_keep_tool_messages`].
    pub prune_keep_tail_messages: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 25,
            max_parallel_tools: 5,
            tool_timeout_ms: 30_000,
            context_budget_chars: 180_000,
            max_protocol_strikes: Some(2),
            max_stagnation_steps: Some(5),
            prune_keep_tool_messages: 10,
            prune_keep_tail_messages: 12,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_frontend_constants() {
        let cfg = AgentConfig::default();
        assert_eq!(cfg.max_iterations, 25);
        assert_eq!(cfg.max_parallel_tools, 5);
        assert_eq!(cfg.tool_timeout_ms, 30_000);
        assert_eq!(cfg.context_budget_chars, 180_000);
        assert_eq!(cfg.max_protocol_strikes, Some(2));
        assert_eq!(
            cfg.max_stagnation_steps,
            Some(5),
            "must mirror MAX_STAGNATION_STEPS from agentLoop.ts"
        );
        assert_eq!(cfg.prune_keep_tool_messages, 10);
        assert_eq!(cfg.prune_keep_tail_messages, 12);
    }
}
