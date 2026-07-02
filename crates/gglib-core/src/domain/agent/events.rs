//! [`AgentEvent`] and [`LlmStreamEvent`] — observable events in the agentic loop.

use serde::Serialize;

use super::tool_types::{ToolCall, ToolResult};
use crate::normalize::NormalizationErrorKind;

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
        /// Human-readable tool name (prefix stripped, title-cased).
        display_name: String,
        /// One-line argument summary (e.g. file path, search pattern).
        args_summary: Option<String>,
    },

    /// A tool execution has completed (success or failure).
    ToolCallComplete {
        /// Name of the tool that was executed.
        tool_name: String,
        /// The outcome of the tool execution.
        result: ToolResult,
        /// Time spent waiting for a concurrency permit (semaphore), in milliseconds.
        wait_ms: u64,
        /// Wall-clock time taken to execute the tool after acquiring the permit,
        /// in milliseconds.
        execute_duration_ms: u64,
        /// Human-readable tool name (prefix stripped, title-cased).
        display_name: String,
        /// Human-readable duration (e.g. "125ms", "1.5s").
        duration_display: String,
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

    /// Prompt-processing progress from the LLM backend.
    ///
    /// Emitted during the pre-fill phase when llama-server is streaming
    /// `prompt_progress` frames.  Surfaces token-level progress so the UI
    /// can show "processing prompt: 2048 / 8192 tokens".
    PromptProgress {
        /// Number of tokens processed so far.
        processed: u32,
        /// Total number of tokens in the prompt.
        total: u32,
        /// Number of tokens served from KV cache (already processed).
        cached: u32,
        /// Elapsed wall-clock time in milliseconds since processing began.
        time_ms: u64,
    },

    /// A non-fatal system-level warning surfaced by the loop itself.
    ///
    /// Unlike [`AgentEvent::Error`], a `SystemWarning` does **not** terminate
    /// the loop — it informs the user that the loop encountered a recoverable
    /// condition (e.g. the model requested more parallel tool calls than the
    /// configured limit, and the loop is auto-retrying with a synthetic
    /// error fed back to the model).
    ///
    /// `suggested_action`, when present, contains an actionable hint the UI
    /// can render verbatim (e.g. a CLI command to permanently raise a limit).
    SystemWarning {
        /// Human-readable description of the recoverable condition.
        message: String,
        /// Optional actionable hint (e.g. CLI command) the UI can show to the
        /// user to permanently address the cause.
        #[serde(skip_serializing_if = "Option::is_none")]
        suggested_action: Option<String>,
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

    /// Prompt-processing progress from llama-server.
    ///
    /// Emitted when the request includes `return_progress: true`.  These
    /// frames arrive during the pre-fill phase (before any `TextDelta`),
    /// giving real-time visibility into how far along token ingestion is.
    PromptProgress {
        /// Number of tokens processed so far.
        processed: u32,
        /// Total number of tokens in the prompt.
        total: u32,
        /// Number of tokens served from KV cache (already processed).
        cached: u32,
        /// Elapsed wall-clock time in milliseconds since processing began.
        time_ms: u64,
    },

    /// Token usage totals for the completed response.
    ///
    /// Emitted when the request includes `stream_options.include_usage:
    /// true` (see `gglib_proxy`'s `inject_streaming_body_overrides`).
    /// Per the `OpenAI` streaming convention this arrives as a single
    /// trailing chunk with an empty/absent `choices` array, immediately
    /// before the `[DONE]` sentinel — never mixed with `TextDelta` or
    /// `ToolCallDelta` events. Consumers that care about real token counts
    /// (e.g. clients feeding a context-window UI widget) read this event;
    /// everyone else can ignore it.
    Usage {
        /// Number of tokens in the prompt.
        prompt_tokens: u32,
        /// Number of tokens generated in the completion.
        completion_tokens: u32,
        /// Total tokens (`prompt_tokens + completion_tokens`).
        total_tokens: u32,
    },

    /// Signals the end of the stream.
    ///
    /// Every conforming stream must end with exactly one `Done` item.
    Done {
        /// The OpenAI-compatible finish reason (e.g. `"stop"`, `"tool_calls"`,
        /// `"length"`).
        finish_reason: String,
    },

    /// An upstream error reported inline, mid-stream.
    ///
    /// Some OpenAI-compatible servers (including llama.cpp) can emit a bare
    /// `{"error": {...}}` frame in the middle of an otherwise-successful SSE
    /// response — e.g. a context-length overflow discovered only once
    /// generation is underway — rather than failing the initial HTTP
    /// response. Without special handling this frame carries no `choices`
    /// array and would otherwise be silently discarded by parsers that treat
    /// "no choices" as an empty keepalive/heartbeat frame.
    ///
    /// `error_type` and `code` default to `"server_error"`/`"upstream_error"`
    /// respectively when the upstream frame omits them, mirroring the
    /// existing convention in `gglib_proxy::forward`'s pre-flight upstream
    /// error handling.
    UpstreamError {
        /// Human-readable error message.
        message: String,
        /// Upstream `error.type` (or `"server_error"` if absent).
        error_type: String,
        /// Upstream `error.code` (or `"upstream_error"` if absent).
        code: String,
    },

    /// A non-fatal normalization issue surfaced by the
    /// [`crate::normalize`] layer.
    ///
    /// Emitted when a dialect-specific parser detects malformed markup
    /// (e.g. a Qwen `<tool_call>` whose body is not valid JSON, or a tag
    /// that the stream ended without closing).  The stream is **not**
    /// aborted; the offending bytes are simply discarded or surfaced via
    /// this event so consumers can log diagnostics.
    NormalizationError {
        /// Structured detail about what went wrong.
        kind: NormalizationErrorKind,
        /// Short, human-readable excerpt of the offending input.
        raw: String,
    },
}

// =============================================================================
// Channel sizing
// =============================================================================

/// [`tokio::sync::mpsc`] channel capacity for streaming [`AgentEvent`]s
/// produced by a single [`crate::ports::AgentLoopPort::run`] call.
///
/// Sized so that a full run at the **maximum ceiling configuration**
/// (`MAX_ITERATIONS_CEILING` × (`MAX_PARALLEL_TOOLS_CEILING` × 2 + 1) + 1
/// structural events ≈ 5 051 with `MAX_ITERATIONS_CEILING = 50` and
/// `MAX_PARALLEL_TOOLS_CEILING = 50`) fits without back-pressure on the hot
/// streaming path.  Any value ≥ 5 051 satisfies the structural ceiling test;
/// 8 192 leaves comfortable headroom for `TextDelta` bursts.
///
/// Filling the channel causes `tx.send().await` to back-pressure on every
/// token, with measurable latency impact, so the constant is set well above
/// the hard floor.
///
/// 8 192 fits comfortably in under a megabyte of memory per active agent
/// session.
///
/// All callers (SSE handlers, CLI REPL) should use this constant instead of
/// a magic literal.
pub const AGENT_EVENT_CHANNEL_CAPACITY: usize = 8_192;

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
            display_name: "Search".into(),
            args_summary: None,
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
