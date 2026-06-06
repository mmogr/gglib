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
/// 50 concurrent tools per iteration is far beyond any practical need and
/// prevents thread-pool saturation from crafted requests.  Modern reasoning
/// models occasionally request large parallel batches (10–25 calls); the
/// ceiling must comfortably exceed the default to leave headroom for users
/// who legitimately want to raise the limit.
pub const MAX_PARALLEL_TOOLS_CEILING: usize = 50;

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

/// Hard floor on `context_budget_chars` (100 characters).
///
/// A budget below this threshold would cause the pruner to discard virtually
/// all context, leaving the LLM with no meaningful history to reason about.
pub const MIN_CONTEXT_BUDGET_CHARS: usize = 100;

/// Default value for [`AgentConfig::max_iterations`].
///
/// Mirrors `DEFAULT_MAX_TOOL_ITERS = 25` from the TypeScript frontend.
/// Used both in [`AgentConfig::default`] and in [`super::events::AGENT_EVENT_CHANNEL_CAPACITY`]
/// so the channel size automatically scales with the iteration ceiling.
pub const DEFAULT_MAX_ITERATIONS: usize = 25;

/// Default value for [`AgentConfig::max_parallel_tools`].
///
/// Set to 25 to comfortably accommodate modern reasoning models (Qwen3-MoE,
/// DeepSeek-R1, etc.) that routinely request 6–10 parallel tool calls per
/// turn during exploration-heavy tasks (e.g. codebase reviews).  An overflow
/// is no longer fatal — the loop now soft-recovers by injecting a synthetic
/// tool error and asking the model to retry with a smaller batch — but a
/// generous default avoids triggering that recovery path under normal load.
///
/// Used both in [`AgentConfig::default`] and in [`super::events::AGENT_EVENT_CHANNEL_CAPACITY`]
/// so the channel size accounts for the correct number of concurrent tool events.
pub const DEFAULT_MAX_PARALLEL_TOOLS: usize = 25;

/// Default value for [`AgentConfig::max_stagnation_steps`].
///
/// The agent loop aborts when the same assistant text has been seen more
/// than this many times, preventing infinite stagnant output.
pub const DEFAULT_MAX_STAGNATION_STEPS: usize = 5;

/// Hard ceiling on [`AgentConfig::max_observation_steps`] accepted from
/// external callers.
///
/// Prevents an API or CLI caller from setting `max_observation_steps` to an
/// arbitrarily large value, which would silently neutralise the observation
/// guard and allow a confused agent to call observation tools indefinitely.
/// 100 observation-only iterations is far more than any legitimate browsing
/// task requires.
pub const MAX_OBSERVATION_STEPS_CEILING: usize = 100;

/// Default value for [`AgentConfig::max_observation_steps`].
///
/// An exploratory-tool-only batch (every call matches a pattern in
/// [`AgentConfig::observation_tools`]) may repeat up to this many times
/// before loop detection fires.  15 is generous for multi-page browsing,
/// multi-directory walking, and paginated API tasks while still catching
/// a genuinely confused agent within a reasonable token budget.
pub const DEFAULT_MAX_OBSERVATION_STEPS: usize = 15;

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

    // -------------------------------------------------------------------------
    // Dual-threshold observation guard
    // -------------------------------------------------------------------------
    //
    // Standard loop detection (max_repeated_batch_steps) hashes tool names and
    // arguments to detect stuck cycles.  Observation-only tools (e.g. browser
    // snapshots, screenshots) legitimately repeat with identical signatures
    // because they take no meaningful arguments, yet return completely different
    // page content on each call.  These two fields allow a separate, higher
    // threshold to be applied when every tool in a batch is classified as an
    // exploratory tool, preventing false-positive loop aborts during ReAct
    // observation and navigation cycles while still eventually catching a
    // genuinely confused agent.
    /// Substring/suffix patterns used to classify tools as **exploratory**.
    ///
    /// "Exploratory" tools are those that drive progress by repeatedly
    /// querying or traversing a stateful source — page snapshots, navigation,
    /// clicks, file reads, directory listings, API pagination calls, etc.
    /// Their repeated invocation with identical arguments is a legitimate
    /// ReAct pattern, not a stuck loop.
    ///
    /// A tool call whose **lowercased** name satisfies
    /// `name.ends_with(pattern) || name.contains(pattern)` for any pattern in
    /// this list is classified as exploratory.  When **every** call in a batch
    /// matches, [`Self::max_observation_steps`] is applied as the loop
    /// detection threshold instead of [`Self::max_repeated_batch_steps`].
    ///
    /// **Matching semantics** — substring/suffix rather than exact string — are
    /// intentional: MCP servers routinely prepend namespace prefixes to tool
    /// names (e.g. `playwright_mcp_browser_snapshot`), so exact matching would
    /// require users to enumerate every vendor variant.  The pattern `"navigate"`
    /// matches `browser_navigate`, `db_navigate`, `fs_navigate`, etc.
    ///
    /// **BYO-MCP:** users connecting custom MCP servers should extend or replace
    /// this list via [`AgentConfig::from_user_params`] to include their own
    /// exploratory tool name fragments (e.g. `"get_dom"`, `"fetch_page"`,
    /// `"list_dir"`).
    ///
    /// An empty list means no tools are ever classified as exploratory;
    /// the standard [`Self::max_repeated_batch_steps`] threshold applies to all
    /// batches.
    ///
    /// Default: `["snapshot", "screenshot", "read_page", "navigate", "click"]`.
    pub observation_tools: Vec<String>,

    /// Maximum number of times an exploratory-tool-only batch may repeat
    /// before loop detection fires.
    ///
    /// Applied **instead of** [`Self::max_repeated_batch_steps`] when every
    /// tool call in the current batch matches a pattern in
    /// [`Self::observation_tools`].  A higher value (default: 15) gives the
    /// agent room to browse multiple pages, walk directory trees, or paginate
    /// through API results while still eventually aborting a genuinely confused
    /// agent before it exhausts the token budget.
    ///
    /// **Mixed batches** (at least one non-exploratory tool alongside an
    /// exploratory one) always fall back to [`Self::max_repeated_batch_steps`]
    /// — the conservative choice.
    ///
    /// Clamped to [`MAX_OBSERVATION_STEPS_CEILING`] when supplied via
    /// [`AgentConfig::from_user_params`] to prevent API callers from providing
    /// a value large enough to neutralise the guard.
    ///
    /// Set to `None` to disable the elevated threshold entirely; exploratory
    /// batches then use [`Self::max_repeated_batch_steps`] like any other batch.
    ///
    /// Default: `Some(15)`.
    pub max_observation_steps: Option<usize>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: DEFAULT_MAX_ITERATIONS,
            max_parallel_tools: DEFAULT_MAX_PARALLEL_TOOLS,
            tool_timeout_ms: 30_000,
            context_budget_chars: 180_000,
            max_repeated_batch_steps: Some(2),
            max_stagnation_steps: Some(DEFAULT_MAX_STAGNATION_STEPS),
            prune_keep_tool_messages: 10,
            prune_keep_tail_messages: 12,
            observation_tools: vec![
                "snapshot".into(),
                "screenshot".into(),
                "read_page".into(),
                "navigate".into(),
                "click".into(),
            ],
            max_observation_steps: Some(DEFAULT_MAX_OBSERVATION_STEPS),
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
    /// `context_budget_chars` must be >= [`MIN_CONTEXT_BUDGET_CHARS`] — a value
    /// below the floor would cause the pruner to discard virtually all context.
    #[error("context_budget_chars must be >= {MIN_CONTEXT_BUDGET_CHARS}, got {0}")]
    ContextBudgetTooLow(usize),
}

impl AgentConfig {
    /// Build an `AgentConfig` from user-supplied overrides.
    ///
    /// Each `Some` numeric value is clamped to the safe `[floor, ceiling]`
    /// range before assignment; `None` fields retain their [`Default`] values.
    /// The result is validated before returning.
    ///
    /// This is the **single entry-point** for HTTP, Tauri, and CLI callers,
    /// eliminating duplicated clamping logic at every call site.
    ///
    /// # Observation-tool parameters
    ///
    /// - `observation_tools: Some(vec)` — **replaces** the default pattern list
    ///   entirely.  Pass the complete list you want, including any defaults you
    ///   wish to preserve.  `Some(vec![])` disables observation classification
    ///   (standard threshold applies to all batches).  `None` keeps the
    ///   built-in defaults (`["snapshot", "screenshot", "read_page"]`).
    ///
    /// - `max_observation_steps: Some(n)` — clamped to
    ///   `[1, MAX_OBSERVATION_STEPS_CEILING]`.  `None` keeps the built-in
    ///   default of `Some(10)`.
    ///
    /// # Errors
    ///
    /// Returns `Err(AgentConfigError)` if the clamped config violates any
    /// invariant (defense-in-depth — should never happen given the clamping).
    pub fn from_user_params(
        max_iterations: Option<usize>,
        max_parallel_tools: Option<usize>,
        tool_timeout_ms: Option<u64>,
        observation_tools: Option<Vec<String>>,
        max_observation_steps: Option<usize>,
    ) -> Result<Self, AgentConfigError> {
        let mut cfg = Self::default();
        if let Some(n) = max_iterations {
            cfg.max_iterations = n.clamp(1, MAX_ITERATIONS_CEILING);
        }
        if let Some(n) = max_parallel_tools {
            cfg.max_parallel_tools = n.clamp(1, MAX_PARALLEL_TOOLS_CEILING);
        }
        if let Some(ms) = tool_timeout_ms {
            cfg.tool_timeout_ms = ms.clamp(MIN_TOOL_TIMEOUT_MS, MAX_TOOL_TIMEOUT_MS_CEILING);
        }
        if let Some(tools) = observation_tools {
            cfg.observation_tools = tools;
        }
        if let Some(n) = max_observation_steps {
            cfg.max_observation_steps = Some(n.clamp(1, MAX_OBSERVATION_STEPS_CEILING));
        }
        cfg.validated()
    }

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
        if self.context_budget_chars < MIN_CONTEXT_BUDGET_CHARS {
            return Err(AgentConfigError::ContextBudgetTooLow(
                self.context_budget_chars,
            ));
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
        assert_eq!(
            cfg.observation_tools,
            vec!["snapshot", "screenshot", "read_page", "navigate", "click"],
            "default exploratory patterns must cover common snapshot, navigation, and click tools"
        );
        assert_eq!(
            cfg.max_observation_steps,
            Some(DEFAULT_MAX_OBSERVATION_STEPS),
            "must match DEFAULT_MAX_OBSERVATION_STEPS"
        );
    }

    #[test]
    fn default_config_passes_validation() {
        assert!(AgentConfig::default().validated().is_ok());
    }

    #[test]
    fn zero_max_iterations_rejected() {
        let cfg = AgentConfig {
            max_iterations: 0,
            ..Default::default()
        };
        assert_eq!(
            cfg.validated().unwrap_err(),
            AgentConfigError::MaxIterationsZero(0),
        );
    }

    #[test]
    fn zero_max_parallel_tools_rejected() {
        let cfg = AgentConfig {
            max_parallel_tools: 0,
            ..Default::default()
        };
        assert_eq!(
            cfg.validated().unwrap_err(),
            AgentConfigError::MaxParallelToolsZero(0),
        );
    }

    #[test]
    fn tool_timeout_below_floor_rejected() {
        let cfg = AgentConfig {
            tool_timeout_ms: MIN_TOOL_TIMEOUT_MS - 1,
            ..Default::default()
        };
        assert_eq!(
            cfg.validated().unwrap_err(),
            AgentConfigError::ToolTimeoutTooLow(MIN_TOOL_TIMEOUT_MS - 1),
        );
    }

    #[test]
    fn tool_timeout_at_floor_accepted() {
        let cfg = AgentConfig {
            tool_timeout_ms: MIN_TOOL_TIMEOUT_MS,
            ..Default::default()
        };
        assert!(cfg.validated().is_ok());
    }

    #[test]
    fn context_budget_below_floor_rejected() {
        let cfg = AgentConfig {
            context_budget_chars: MIN_CONTEXT_BUDGET_CHARS - 1,
            ..Default::default()
        };
        assert_eq!(
            cfg.validated().unwrap_err(),
            AgentConfigError::ContextBudgetTooLow(MIN_CONTEXT_BUDGET_CHARS - 1),
        );
    }

    #[test]
    fn context_budget_at_floor_accepted() {
        let cfg = AgentConfig {
            context_budget_chars: MIN_CONTEXT_BUDGET_CHARS,
            ..Default::default()
        };
        assert!(cfg.validated().is_ok());
    }

    #[test]
    fn boundary_values_accepted() {
        let cfg = AgentConfig {
            max_iterations: 1,
            max_parallel_tools: 1,
            tool_timeout_ms: MIN_TOOL_TIMEOUT_MS,
            context_budget_chars: MIN_CONTEXT_BUDGET_CHARS,
            ..Default::default()
        };
        assert!(cfg.validated().is_ok());
    }

    #[test]
    fn from_user_params_clamps_and_validates() {
        // All values within range → accepted as-is.
        let cfg =
            AgentConfig::from_user_params(Some(10), Some(3), Some(5_000), None, None).unwrap();
        assert_eq!(cfg.max_iterations, 10);
        assert_eq!(cfg.max_parallel_tools, 3);
        assert_eq!(cfg.tool_timeout_ms, 5_000);
    }

    #[test]
    fn from_user_params_clamps_extremes() {
        // Zero iterations → clamped to 1.
        let cfg = AgentConfig::from_user_params(Some(0), Some(0), Some(0), None, None).unwrap();
        assert_eq!(cfg.max_iterations, 1);
        assert_eq!(cfg.max_parallel_tools, 1);
        assert_eq!(cfg.tool_timeout_ms, MIN_TOOL_TIMEOUT_MS);
    }

    #[test]
    fn from_user_params_clamps_above_ceiling() {
        let cfg = AgentConfig::from_user_params(
            Some(usize::MAX),
            Some(usize::MAX),
            Some(u64::MAX),
            None,
            None,
        )
        .unwrap();
        assert_eq!(cfg.max_iterations, MAX_ITERATIONS_CEILING);
        assert_eq!(cfg.max_parallel_tools, MAX_PARALLEL_TOOLS_CEILING);
        assert_eq!(cfg.tool_timeout_ms, MAX_TOOL_TIMEOUT_MS_CEILING);
    }

    #[test]
    fn from_user_params_none_keeps_defaults() {
        let cfg = AgentConfig::from_user_params(None, None, None, None, None).unwrap();
        let def = AgentConfig::default();
        assert_eq!(cfg.max_iterations, def.max_iterations);
        assert_eq!(cfg.max_parallel_tools, def.max_parallel_tools);
        assert_eq!(cfg.tool_timeout_ms, def.tool_timeout_ms);
        assert_eq!(cfg.observation_tools, def.observation_tools);
        assert_eq!(cfg.max_observation_steps, def.max_observation_steps);
    }

    #[test]
    fn from_user_params_observation_tools_replaces_defaults() {
        // A non-None observation_tools list replaces the built-in defaults.
        let custom = vec!["get_dom".into(), "fetch_page".into()];
        let cfg =
            AgentConfig::from_user_params(None, None, None, Some(custom.clone()), None).unwrap();
        assert_eq!(cfg.observation_tools, custom);
    }

    #[test]
    fn from_user_params_empty_observation_tools_disables_classification() {
        // Some([]) disables observation classification — no tools ever match.
        let cfg = AgentConfig::from_user_params(None, None, None, Some(vec![]), None).unwrap();
        assert!(cfg.observation_tools.is_empty());
    }

    #[test]
    fn from_user_params_observation_steps_clamped_to_ceiling() {
        let cfg = AgentConfig::from_user_params(None, None, None, None, Some(usize::MAX)).unwrap();
        assert_eq!(
            cfg.max_observation_steps,
            Some(MAX_OBSERVATION_STEPS_CEILING),
        );
    }

    #[test]
    fn from_user_params_observation_steps_clamped_to_floor() {
        // Zero would mean fire on the very first occurrence — clamp to 1.
        let cfg = AgentConfig::from_user_params(None, None, None, None, Some(0)).unwrap();
        assert_eq!(cfg.max_observation_steps, Some(1));
    }

    #[test]
    fn from_user_params_observation_steps_within_range_unchanged() {
        let cfg = AgentConfig::from_user_params(None, None, None, None, Some(15)).unwrap();
        assert_eq!(cfg.max_observation_steps, Some(15));
    }
}
