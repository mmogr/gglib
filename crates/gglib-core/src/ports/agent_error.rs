//! The error that ends an agent loop: [`AgentError`], returned from
//! [`AgentLoopPort::run`](super::AgentLoopPort::run).

use thiserror::Error;

#[cfg(doc)]
use crate::domain::agent::AgentConfig;

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

    /// The loop detected a tool-call signature repeated **back to back and
    /// answered the same way**, indicating the model is stuck rather than
    /// working.
    ///
    /// The `signature` field is a stable hash of the tool-call batch that
    /// recurred, with nothing in between **and getting the same answer back
    /// each time**, more times than
    /// [`AgentConfig::max_repeated_batch_steps`] allows. A batch that repeats
    /// with other work between occurrences is not a cycle and does not raise
    /// this; neither does one whose answers keep changing, which is an agent
    /// polling for output rather than a stuck one.
    ///
    /// It is also raised when a batch that is *not* read-only has been carried
    /// past `max_observation_steps` by changing answers. The two are not
    /// distinguished here: the remedy is the same, and this variant is
    /// mirrored into the proxy's 400 body.
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

    /// The assistant produced the same text content too many times in a short
    /// span of prose turns, indicating a non-tool-calling repetition loop.
    ///
    /// Preserves the FNV-1a hash of the repeated text, the occurrence count
    /// within the window (including baseline), and the configured
    /// `max_stagnation_steps` limit — giving callers structured access to the
    /// stagnation evidence without parsing an error string.
    ///
    /// Detection is windowed: both strictly consecutive repetitions and
    /// A → B → A oscillations are caught inside the window.
    #[error(
        "agent stagnated: same response text seen {count} time(s) in a short span \
         (max_stagnation_steps = {max_steps})"
    )]
    StagnationDetected {
        /// FNV-1a hash of the repeated assistant text, hex-encoded for diagnostics.
        /// Stored as `String` to decouple the public API from the internal u64
        /// representation so callers never need to know the hashing algorithm.
        repeated_text_hash: String,
        /// Total number of times this text has been seen in the session
        /// (including the baseline occurrence).
        count: usize,
        /// The configured stagnation limit at the time of detection.
        max_steps: usize,
    },

    /// The run's first reply had to call a tool
    /// ([`AgentConfig::first_call`]) and did not call it: an upstream that does
    /// not honour the demand. Carries the composer's own words for the
    /// person, which the loop also emitted as its last `error` event.
    #[error("{message}")]
    FirstCallMissing {
        /// [`FirstCall::if_missing`](crate::domain::agent::FirstCall::if_missing).
        message: String,
    },

    /// The model's turn was cut off at its token limit (`finish_reason:
    /// "length"`) while it wrote a tool call, or before it wrote any text.
    /// The call never closed and there is no answer to give, so the run ends
    /// there: nothing of the turn is run or sent back to the model. A turn
    /// cut off in its text is not this: it is taken as if it had stopped.
    #[error("the model's turn was cut off at its token limit before it finished")]
    TurnTooLong {
        /// The tokens the turn took, when the upstream reported them.
        completion_tokens: Option<u32>,
    },

    /// An unrecoverable internal error inside the loop implementation.
    #[error("internal agent error: {0}")]
    Internal(String),
}
