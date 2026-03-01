//! [`AgentConfig`] — configuration for a single agentic loop run.
//!
//! This module also defines the public ceiling constants used by HTTP and CLI
//! callers to clamp untrusted user input to safe values.  Centralising them
//! here ensures a single source of truth across all entry points.

use serde::Serialize;

// =============================================================================
// Ceiling constants — shared across HTTP and CLI callers
// =============================================================================

/// Hard ceiling on `max_iterations` accepted from external callers.
///
/// 50 iterations is generous for real workloads.  Prevents a crafted request
/// from running an unbounded loop at server expense.
pub const MAX_ITERATIONS_CEILING: usize = 50;

/// Hard ceiling on `max_parallel_tools` accepted from external callers.
///
/// 20 concurrent tools per iteration is far beyond any practical need and
/// prevents thread-pool saturation from crafted requests.
pub const MAX_PARALLEL_TOOLS_CEILING: usize = 20;

/// Hard ceiling on `tool_timeout_ms` accepted from external callers (60 s).
///
/// Prevents a crafted request from holding server connections open
/// indefinitely via slow or stalled tool calls.
pub const MAX_TOOL_TIMEOUT_MS_CEILING: u64 = 60_000;

/// Default value for [`AgentConfig::max_iterations`].
///
/// Mirrors `DEFAULT_MAX_TOOL_ITERS = 25` from the TypeScript frontend.
/// Used both in [`AgentConfig::default`] and in [`super::events::AGENT_EVENT_CHANNEL_CAPACITY`]
/// so the channel size automatically scales with the iteration ceiling.
pub const DEFAULT_MAX_ITERATIONS: usize = 25;

/// Default value for [`AgentConfig::max_parallel_tools`].
///
/// Mirrors `MAX_PARALLEL_TOOLS = 5` from the TypeScript frontend.
/// Used both in [`AgentConfig::default`] and in [`super::events::AGENT_EVENT_CHANNEL_CAPACITY`]
/// so the channel size accounts for the correct number of concurrent tool events.
pub const DEFAULT_MAX_PARALLEL_TOOLS: usize = 5;

/// Configuration that governs a single agentic loop run.
///
/// All fields have sensible defaults via [`Default`] that match the historical
/// TypeScript frontend constants (previously in `agentLoop.ts`, now reflected
/// in `streamAgentChat.ts`).
///
/// # Serialisation
///
/// `AgentConfig` is intentionally **not** `Deserialize`.  External callers
/// (HTTP, future config files) must go through a dedicated DTO that exposes
/// only the safe subset of fields.  This prevents accidental exposure of
/// internal tuning knobs (pruning parameters, strike limits, etc.) to
/// untrusted callers.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
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
    /// before the loop is declared stuck and aborted with
    /// [`crate::ports::AgentError::LoopDetected`].
    ///
    /// Frontend constant: `MAX_SAME_SIGNATURE_HITS = 2` in `agentLoop.ts`.
    ///
    /// Set to `None` to disable loop detection entirely (useful in tests that
    /// deliberately repeat the same tool call).
    pub max_repeated_batch_steps: Option<usize>,

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
    #[serde(skip)]
    pub prune_keep_tool_messages: usize,

    /// Number of non-system messages retained during the emergency tail-prune
    /// pass (second pass of context pruning).
    ///
    /// Same rationale as [`Self::prune_keep_tool_messages`].
    #[serde(skip)]
    pub prune_keep_tail_messages: usize,

    /// Whether to include the full accumulated conversation history in
    /// [`crate::ports::AgentRunOutput::history`] on a successful run.
    ///
    /// Defaults to `false`.  HTTP SSE handlers should leave this `false`
    /// (the history is discarded after streaming); CLI callers set this to
    /// `true` so they can feed the history back as `messages` on the next
    /// REPL turn.
    pub return_history: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: DEFAULT_MAX_ITERATIONS,
            max_parallel_tools: DEFAULT_MAX_PARALLEL_TOOLS,
            tool_timeout_ms: 30_000,
            context_budget_chars: 180_000,
            max_repeated_batch_steps: Some(2),
            max_stagnation_steps: Some(5),
            prune_keep_tool_messages: 10,
            prune_keep_tail_messages: 12,
            return_history: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_frontend_constants() {
        let cfg = AgentConfig::default();
        assert_eq!(cfg.max_iterations, DEFAULT_MAX_ITERATIONS);
        assert_eq!(cfg.max_parallel_tools, DEFAULT_MAX_PARALLEL_TOOLS);
        assert_eq!(cfg.tool_timeout_ms, 30_000);
        assert_eq!(cfg.context_budget_chars, 180_000);
        assert_eq!(cfg.max_repeated_batch_steps, Some(2));
        assert_eq!(
            cfg.max_stagnation_steps,
            Some(5),
            "must mirror MAX_STAGNATION_STEPS from agentLoop.ts"
        );
        assert_eq!(cfg.prune_keep_tool_messages, 10);
        assert_eq!(cfg.prune_keep_tail_messages, 12);
        assert!(!cfg.return_history, "return_history must default to false");
    }
}
