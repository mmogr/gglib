//! [`AgentConfig`] — configuration for a single agentic loop run.
//!
//! This module also defines the public ceiling constants used by HTTP and CLI
//! callers to clamp untrusted user input to safe values.  Centralising them
//! here ensures a single source of truth across all entry points.

use serde::Serialize;
use thiserror::Error;

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

/// Hard floor on `tool_timeout_ms` accepted from external callers (100 ms).
///
/// A value of 0 would silently time out every tool call immediately, making
/// tool calling unusable without a clear error.  100 ms is still very tight
/// but allows intentionally fast tools (health checks, no-ops in tests).
pub const MIN_TOOL_TIMEOUT_MS: u64 = 100;

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
    /// **Dual-purpose:** this value is used both as the `Semaphore` concurrency
    /// cap in `tool_execution` (limiting simultaneous in-flight calls) *and* as
    /// an upper bound on the batch size the model may request in a single turn.
    /// If the model emits more tool calls than this limit, the loop terminates
    /// with [`AgentError::ParallelToolLimitExceeded`] rather than silently
    /// serialising them.  Setting this to `1` means the model may only request
    /// **one** tool call per turn; two calls in a single response will abort the
    /// loop, not run them sequentially.
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
    /// Frontend constant: `MAX_SAME_SIGNATURE_HITS = 2` in `streamAgentChat.ts`.
    ///
    /// Set to `None` to disable loop detection entirely (useful in tests that
    /// deliberately repeat the same tool call).
    pub max_repeated_batch_steps: Option<usize>,

    /// Session-wide occurrence limit for identical assistant text before the
    /// loop is considered stagnant and aborted with
    /// [`crate::ports::AgentError::StagnationDetected`].
    ///
    /// **Semantics:** Each occurrence of the same response text increments a
    /// session counter.  The error fires when the counter **after**
    /// incrementing exceeds `max_stagnation_steps`.  With the default value
    /// of `5`, stagnation triggers on the **sixth** identical occurrence.
    /// With `max_stagnation_steps = 0`, the error fires on the **very first**
    /// occurrence of any repeated text.
    ///
    /// Frontend constant: `MAX_STAGNATION_STEPS = 5` in `streamAgentChat.ts`.
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
        }
    }
}

// =============================================================================
// Validation
// =============================================================================

/// Error returned when [`AgentConfig::validated`] detects an invalid field.
///
/// Each variant names the exact invariant that was violated and carries the
/// offending value so callers (HTTP handlers, CLI) can surface a precise
/// diagnostic without re-inspecting the config.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentConfigError {
    /// `max_iterations` must be ≥ 1 — zero would make the loop exit
    /// immediately as `MaxIterationsReached(0)` without ever calling the LLM.
    #[error("max_iterations must be >= 1, got {0}")]
    MaxIterationsZero(usize),

    /// `max_parallel_tools` must be ≥ 1 — zero would deadlock the
    /// `Semaphore` used for tool-call concurrency (no permit can ever be
    /// acquired).
    #[error("max_parallel_tools must be >= 1, got {0} (0 would deadlock the semaphore)")]
    MaxParallelToolsZero(usize),

    /// `tool_timeout_ms` must be ≥ [`MIN_TOOL_TIMEOUT_MS`] — a value below
    /// the floor would silently time out every tool call, making tool
    /// calling unusable without a clear error.
    #[error("tool_timeout_ms must be >= {MIN_TOOL_TIMEOUT_MS}, got {0}")]
    ToolTimeoutTooLow(u64),
}

impl AgentConfig {
    /// Validate all fields that could cause the agent loop to malfunction.
    ///
    /// Call this after constructing an `AgentConfig` from untrusted input.
    /// The [`Default`] implementation is always valid; this acts as a safety
    /// net for values assembled by HTTP DTOs or CLI argument parsing.
    ///
    /// # Errors
    ///
    /// Returns `Err(AgentConfigError)` if any field violates its invariant.
    pub fn validated(self) -> Result<Self, AgentConfigError> {
        if self.max_iterations < 1 {
            return Err(AgentConfigError::MaxIterationsZero(self.max_iterations));
        }
        if self.max_parallel_tools < 1 {
            return Err(AgentConfigError::MaxParallelToolsZero(
                self.max_parallel_tools,
            ));
        }
        if self.tool_timeout_ms < MIN_TOOL_TIMEOUT_MS {
            return Err(AgentConfigError::ToolTimeoutTooLow(self.tool_timeout_ms));
        }
        Ok(self)
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
            "must mirror MAX_STAGNATION_STEPS from streamAgentChat.ts"
        );
        assert_eq!(cfg.prune_keep_tool_messages, 10);
        assert_eq!(cfg.prune_keep_tail_messages, 12);
    }

    #[test]
    fn default_config_passes_validation() {
        assert!(AgentConfig::default().validated().is_ok());
    }

    #[test]
    fn zero_max_iterations_rejected() {
        let mut cfg = AgentConfig::default();
        cfg.max_iterations = 0;
        assert_eq!(
            cfg.validated().unwrap_err(),
            AgentConfigError::MaxIterationsZero(0),
        );
    }

    #[test]
    fn zero_max_parallel_tools_rejected() {
        let mut cfg = AgentConfig::default();
        cfg.max_parallel_tools = 0;
        assert_eq!(
            cfg.validated().unwrap_err(),
            AgentConfigError::MaxParallelToolsZero(0),
        );
    }

    #[test]
    fn tool_timeout_below_floor_rejected() {
        let mut cfg = AgentConfig::default();
        cfg.tool_timeout_ms = MIN_TOOL_TIMEOUT_MS - 1;
        assert_eq!(
            cfg.validated().unwrap_err(),
            AgentConfigError::ToolTimeoutTooLow(MIN_TOOL_TIMEOUT_MS - 1),
        );
    }

    #[test]
    fn tool_timeout_at_floor_accepted() {
        let mut cfg = AgentConfig::default();
        cfg.tool_timeout_ms = MIN_TOOL_TIMEOUT_MS;
        assert!(cfg.validated().is_ok());
    }

    #[test]
    fn boundary_values_accepted() {
        let mut cfg = AgentConfig::default();
        cfg.max_iterations = 1;
        cfg.max_parallel_tools = 1;
        cfg.tool_timeout_ms = MIN_TOOL_TIMEOUT_MS;
        assert!(cfg.validated().is_ok());
    }
}
