//! [`AgentEvent`] and [`LlmStreamEvent`] — observable events in the agentic loop.

use serde::Serialize;

use super::tool_types::{ToolCall, ToolResult};

// =============================================================================
// Agent events (SSE stream units)
// =============================================================================

/// An observable event emitted by the agentic loop.
///
/// These events are the unit of SSE emission: every state change in the loop
/// produces exactly one variant. Axum SSE handlers serialise these to
/// `data: <json>\n\n` frames; CLI consumers may log or render them directly.
///
/// # Serde tag
///
/// `#[serde(tag = "type", rename_all = "snake_case")]` produces e.g.
/// `{"type":"tool_call_start","tool_call":{...}}`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// An incremental text fragment from the model's response.
    TextDelta {
        /// The new text fragment (append to the current buffer).
        content: String,
    },

    /// An incremental reasoning/thinking fragment from the model (`CoT` tokens).
    ///
    /// Emitted by reasoning-capable models (e.g. `DeepSeek` R1, `QwQ`) that expose
    /// their chain-of-thought via a separate `reasoning_content` SSE field.
    /// These fragments are forwarded to the UI as they arrive but are **not**
    /// included in the conversation history sent back to the model.
    ReasoningDelta {
        /// The new reasoning fragment (append to the current reasoning buffer).
        content: String,
    },

    /// The model has requested execution of a tool.
    ToolCallStart {
        /// The tool call that is about to be dispatched.
        tool_call: ToolCall,
    },

    /// A tool execution has completed (success or failure).
    ToolCallComplete {
        /// The outcome of the tool execution.
        result: ToolResult,
        /// Time spent waiting for a concurrency permit (semaphore), in milliseconds.
        wait_ms: u64,
        /// Wall-clock time taken to execute the tool after acquiring the permit,
        /// in milliseconds.
        execute_duration_ms: u64,
    },

    /// One full LLM→tool-execution cycle has completed.
    IterationComplete {
        /// The 1-based iteration index that just finished.
        iteration: usize,
        /// Number of tool calls executed during this iteration.
        tool_calls: usize,
    },

    /// The loop has concluded and produced a definitive answer.
    FinalAnswer {
        /// The complete final response text.
        content: String,
    },

    /// A fatal error has terminated the loop.
    Error {
        /// Human-readable description of the failure.
        message: String,
    },
}

// =============================================================================
// LLM stream events (consumed by LlmCompletionPort implementors)
// =============================================================================

/// A single event produced by a streaming LLM response.
///
/// These low-level events are the currency of [`crate::ports::LlmCompletionPort`];
/// they are parsed by adapter crates from raw SSE frames and handed to
/// `gglib-agent`'s stream collector, which:
///
/// - Forwards [`TextDelta`](LlmStreamEvent::TextDelta) items directly to the
///   caller's [`AgentEvent`] channel so text appears in real time.
/// - Accumulates [`ToolCallDelta`](LlmStreamEvent::ToolCallDelta) fragments
///   until the stream ends, then assembles them into [`ToolCall`] values.
/// - Waits for [`Done`](LlmStreamEvent::Done) before triggering tool execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmStreamEvent {
    /// An incremental text fragment from the model's response.
    TextDelta {
        /// The new text fragment (append to the running content buffer).
        content: String,
    },

    /// An incremental reasoning/thinking fragment (`CoT` tokens).
    ///
    /// Produced by reasoning-capable models (e.g. `DeepSeek` R1, `QwQ`) when
    /// llama-server is started with `--reasoning-format deepseek`.  The
    /// runtime adapter maps `delta["reasoning_content"]` frames to this
    /// variant; the stream collector forwards them as
    /// [`AgentEvent::ReasoningDelta`] and accumulates them in a separate
    /// buffer that is never sent back to the LLM as context.
    ReasoningDelta {
        /// The new reasoning fragment (append to the current reasoning buffer).
        content: String,
    },

    /// An incremental fragment of a tool-call request.
    ///
    /// The adapter crate streams these before the model has finished
    /// generating the full arguments JSON. The stream collector accumulates
    /// all deltas for a given `index` into a single [`ToolCall`].
    ToolCallDelta {
        /// Zero-based index of the tool call within the current response.
        index: usize,
        /// Call identifier (only present in the first delta for this index).
        id: Option<String>,
        /// Tool name (only present in the first delta for this index).
        name: Option<String>,
        /// Partial arguments JSON string fragment (accumulate with `push_str`).
        arguments: Option<String>,
    },

    /// Signals the end of the stream.
    ///
    /// Every conforming stream must end with exactly one `Done` item.
    Done {
        /// The OpenAI-compatible finish reason (e.g. `"stop"`, `"tool_calls"`,
        /// `"length"`).
        finish_reason: String,
    },
}

// =============================================================================
// Channel sizing
// =============================================================================

/// [`tokio::sync::mpsc`] channel capacity for streaming [`AgentEvent`]s
/// produced by a single [`crate::ports::AgentLoopPort::run`] call.
///
/// Set to a generous static ceiling (4 096) rather than a formula tied to
/// default config values.  The formula-based value (~532 for default config)
/// is too small for callers that use non-default settings such as
/// `max_iterations = 50` + `max_parallel_tools = 20`, which would produce
/// up to ~2 061 structural events per run before any `TextDelta` headroom.
/// Filling the channel causes `tx.send().await` to back-pressure on every
/// token in the hot streaming path, with measurable latency impact.
///
/// 4 096 fits comfortably in a few hundred kilobytes of memory per active
/// agent session and is sufficient for any realistic configuration.
///
/// All callers (SSE handlers, CLI REPL) should use this constant instead of
/// a magic literal.
pub const AGENT_EVENT_CHANNEL_CAPACITY: usize = 4_096;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_event_serde_tag_matches_wire_format() {
        let evt = AgentEvent::FinalAnswer {
            content: "done".into(),
        };
        let json = serde_json::to_value(&evt).unwrap();
        assert_eq!(json["type"], "final_answer");
        assert_eq!(json["content"], "done");
    }

    #[test]
    fn tool_call_start_serialises_correctly() {
        let evt = AgentEvent::ToolCallStart {
            tool_call: ToolCall {
                id: "c1".into(),
                name: "search".into(),
                arguments: serde_json::json!({"q": "rust"}),
            },
        };
        let json = serde_json::to_value(&evt).unwrap();
        assert_eq!(json["type"], "tool_call_start");
        assert_eq!(json["tool_call"]["name"], "search");
    }

    /// [`AGENT_EVENT_CHANNEL_CAPACITY`] must be positive and must be at least
    /// large enough for a full run at the maximum ceiling configuration
    /// (`MAX_ITERATIONS_CEILING` × (`MAX_PARALLEL_TOOLS_CEILING` × 2 + 1) + 1
    /// structural events), so that back-pressure never occurs on the hot
    /// streaming path for any valid configuration.
    #[test]
    fn agent_event_channel_capacity_is_sufficient_for_max_config() {
        use super::super::config::{MAX_ITERATIONS_CEILING, MAX_PARALLEL_TOOLS_CEILING};

        // Minimum structural events for a run at ceiling config
        // (no TextDelta headroom included — this is the hard lower bound).
        let structural_per_iter = MAX_PARALLEL_TOOLS_CEILING * 2 + 1;
        let minimum_structural = MAX_ITERATIONS_CEILING * structural_per_iter + 1;
        assert!(
            AGENT_EVENT_CHANNEL_CAPACITY >= minimum_structural,
            "AGENT_EVENT_CHANNEL_CAPACITY ({AGENT_EVENT_CHANNEL_CAPACITY}) is smaller than \
             the minimum required for ceiling config ({minimum_structural}); \
             increase the constant"
        );
    }
}
