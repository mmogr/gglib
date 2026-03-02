//! Agent loop port traits.
//!
//! Defines the hexagonal-architecture port interfaces for the backend agentic
//! loop. All types used in signatures are from `gglib-core`; no adapter- or
//! crate-specific symbols appear here.
//!
//! # Port hierarchy
//!
//! ```text
//! AgentLoopPort
//!   └── uses ──→ ToolExecutorPort  (to dispatch individual tool calls)
//!   └── emits ──→ Sender<AgentEvent>  (SSE-ready async channel)
//! ```
//!
//! # Error separation
//!
//! | Concern | Type |
//! |---------|------|
//! | Fatal loop failure | [`AgentError`] — returned from [`AgentLoopPort::run`] |
//! | Executor infrastructure failure | `anyhow::Error` — from [`ToolExecutorPort::execute`] |
//! | Tool-level outcome (incl. failures) | [`ToolResult::success`] field — LLM context |
//!
//! A tool result with `success: false` is **not** an error; it is fed back into
//! the conversation so the model can observe and react to the failure.
//! `AgentError` is reserved for conditions where the loop itself cannot continue.

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::domain::agent::{
    AgentConfig, AgentEvent, AgentMessage, ToolCall, ToolDefinition, ToolResult,
};

// =============================================================================
// Error type — fatal loop-level failures only
// =============================================================================

/// Errors that terminate the agentic loop.
///
/// These represent conditions where `AgentLoopPort::run` cannot continue.
/// They do **not** include tool execution failures — those are encoded as
/// `ToolResult { success: false }` and fed back to the LLM as context.
#[derive(Debug, Error)]
pub enum AgentError {
    /// The loop reached [`AgentConfig::max_iterations`] without producing a
    /// final answer.
    #[error("agent loop reached the maximum number of iterations ({0})")]
    MaxIterationsReached(usize),

    /// The loop detected a repeated tool-call signature, indicating the model
    /// is stuck in a cycle.
    ///
    /// The `signature` field is a stable hash of the tool-call batch that was
    /// repeated beyond [`AgentConfig::max_repeated_batch_steps`].
    #[error("tool-call loop detected (repeated signature: {signature})")]
    LoopDetected {
        /// Stable hash of the repeated tool-call batch (for diagnostics).
        signature: String,
    },

    /// The LLM produced more tool calls in a single batch than configured by
    /// [`AgentConfig::max_parallel_tools`].
    ///
    /// This is a model protocol violation: the LLM returned more concurrent
    /// calls than the loop is configured to dispatch.  The loop aborts rather
    /// than silently truncating the batch, because partial execution could
    /// leave the model with an incoherent view of which calls were handled.
    #[error("LLM requested {count} tool calls in one batch, exceeds max_parallel_tools ({limit})")]
    ParallelToolLimitExceeded {
        /// Number of tool calls the LLM returned.
        count: usize,
        /// The configured maximum ([`AgentConfig::max_parallel_tools`]).
        limit: usize,
    },

    /// The assistant produced the same text content for too many consecutive
    /// iterations, indicating a non-tool-calling repetition loop.
    ///
    /// Preserves the FNV-1a hash of the repeated text, the total session-wide
    /// occurrence count (including baseline), and the configured
    /// `max_stagnation_steps` limit — giving callers structured access to the
    /// stagnation evidence without parsing an error string.
    ///
    /// Detection is session-wide: both strictly consecutive repetitions and
    /// A → B → A oscillations are caught.
    #[error(
        "agent stagnated: same response text seen {count} time(s) in this session \
         (max_stagnation_steps = {max_steps})"
    )]
    StagnationDetected {
        /// FNV-1a hash of the repeated assistant text (for diagnostics).
        repeated_text_hash: u64,
        /// Total number of times this text has been seen in the session
        /// (including the baseline occurrence).
        count: usize,
        /// The configured stagnation limit at the time of detection.
        max_steps: usize,
    },

    /// An unrecoverable internal error inside the loop implementation.
    #[error("internal agent error: {0}")]
    Internal(String),
}

// =============================================================================
// AgentRunOutput — structured return value for a successful run
// =============================================================================

/// Output returned by a successful [`AgentLoopPort::run`] invocation.
///
/// Using a named struct instead of a bare tuple keeps call sites
/// self-documenting and allows new fields to be added without breaking
/// existing destructures.
#[derive(Debug)]
pub struct AgentRunOutput {
    /// The final answer text produced by the agent.
    pub answer: String,
    /// Full accumulated conversation history: the caller-supplied messages
    /// **plus** every assistant and tool-result message appended during the
    /// loop, including the final assistant reply.
    ///
    /// Safe to pass directly as the `messages` argument for the next turn.
    pub history: Vec<AgentMessage>,
    /// Number of loop iterations consumed before the agent produced its final
    /// answer.  Always ≥ 1.  Useful for logging and telemetry.
    pub total_iterations: usize,
}

// =============================================================================
// ToolExecutorPort
// =============================================================================

/// Port: dispatches tool calls to the underlying execution backend.
///
/// # Implementing this trait
///
/// ```ignore
/// use gglib_core::ports::{AgentError, ToolExecutorPort};
/// use gglib_core::domain::{ToolCall, ToolDefinition, ToolResult};
///
/// struct McpToolExecutor { /* ... */ }
///
/// #[async_trait::async_trait]
/// impl ToolExecutorPort for McpToolExecutor {
///     async fn list_tools(&self) -> Vec<ToolDefinition> { /* ... */ }
///
///     async fn execute(&self, call: &ToolCall) -> Result<ToolResult, anyhow::Error> {
///         // Call the MCP client; convert McpToolResult → ToolResult.
///         // Return Err(_) only if the infrastructure itself is unavailable.
///     }
/// }
/// ```
///
/// # Error contract
///
/// - Returns `Ok(ToolResult { success: false, .. })` when the tool ran but
///   produced an application-level error (wrong args, resource not found, etc.).
///   The loop implementation **must** feed this back to the LLM as context.
/// - Returns `Err(anyhow::Error)` only when the executor infrastructure is
///   unavailable (e.g. MCP process died, network unreachable).  The loop
///   implementation converts this into `ToolResult { success: false, content:
///   "executor unavailable: …" }` so the LLM still receives context.
#[async_trait]
pub trait ToolExecutorPort: Send + Sync {
    /// Return all tool definitions available in this executor.
    ///
    /// Called once per agent `run` invocation to build the tool list sent to
    /// the LLM.
    async fn list_tools(&self) -> Vec<ToolDefinition>;

    /// Execute a single tool call.
    ///
    /// Returns `Err` only for infrastructure failures (see error contract above).
    async fn execute(&self, call: &ToolCall) -> Result<ToolResult, anyhow::Error>;
}

// =============================================================================
// AgentLoopPort
// =============================================================================

/// Port: drives the full backend agentic loop.
///
/// # Usage
///
/// ```ignore
/// use tokio::sync::mpsc;
/// use gglib_core::ports::AgentLoopPort;
/// use gglib_core::domain::{AgentConfig, AgentEvent, AgentMessage};
///
/// async fn run_loop(agent: &dyn AgentLoopPort) {
///     let (tx, mut rx) = mpsc::channel::<AgentEvent>(64);
///
///     // Spawn a task to consume the event stream (e.g. SSE or logging).
///     tokio::spawn(async move {
///         while let Some(event) = rx.recv().await {
///             println!("{:?}", event);
///         }
///         // rx.recv() returns None when tx is dropped (loop ended).
///     });
///
///     let messages = vec![AgentMessage::User { content: "Hello".into() }];
///     let output = agent.run(messages, AgentConfig::default(), tx).await?;
///     println!("Final: {}", output.answer);
///     // `history` contains the full accumulated message list including all
///     // assistant and tool-result messages appended during the loop — safe
///     // to pass directly as the `messages` argument for the next turn.
/// }
/// ```
///
/// # Channel ownership and stream termination
///
/// `events` is taken **by value**. When `run` returns (whether `Ok` or `Err`)
/// the `Sender` is dropped, which closes the channel and signals `None` to
/// the `Receiver`. Axum SSE handlers and CLI consumers can rely on this to
/// know the stream has ended without needing an explicit sentinel event.
#[async_trait]
pub trait AgentLoopPort: Send + Sync {
    /// Execute the agentic loop and return the final answer.
    ///
    /// # Parameters
    ///
    /// * `messages` — The initial conversation history (system prompt + user
    ///   message at minimum).
    /// * `config` — Loop control parameters (iteration limits, timeouts, etc.).
    /// * `tx` — Async channel over which the loop streams [`AgentEvent`]s.
    ///   Taken by value; dropped on completion to close the SSE stream.
    ///
    /// # Returns
    ///
    /// * `Ok(AgentRunOutput)` — The final answer and full accumulated message
    ///   history (safe to pass back as `messages` on the next turn).
    /// * `Err(AgentError)` — A fatal loop-level failure (max iterations reached,
    ///   loop detection, stagnation, or internal error).  No partial history is
    ///   returned on failure; the caller's existing history is left intact.
    async fn run(
        &self,
        messages: Vec<AgentMessage>,
        config: AgentConfig,
        tx: mpsc::Sender<AgentEvent>,
    ) -> Result<AgentRunOutput, AgentError>;
}
