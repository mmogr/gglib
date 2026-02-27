//! Agent loop domain types.
//!
//! These types define the core abstractions for the backend agentic loop.
//! They are pure domain primitives: no LLM backend references, no MCP types,
//! no infrastructure concerns.
//!
//! # Design Principles
//!
//! - [`AgentMessage`] is a closed enum so the type system prevents invalid states
//!   (e.g. a `User` message carrying `tool_calls`).
//! - [`ToolDefinition`] is a dedicated type — adapter layers convert `McpTool →
//!   ToolDefinition`; the agent domain must not depend on MCP domain types.
//! - [`ToolResult`] with `success: false` is **context for the LLM**, not an error;
//!   tool failures are fed back into the conversation so the model can reason about
//!   them and retry or adjust its approach.
//! - [`AgentEvent`] is the unit of SSE emission; every observable state change in
//!   the loop corresponds to exactly one variant.

use serde::{Deserialize, Serialize};

// =============================================================================
// Configuration
// =============================================================================

/// Configuration that governs a single agentic loop run.
///
/// All fields have sensible defaults via [`Default`] that match the constants
/// used in the TypeScript frontend (`src/hooks/useGglibRuntime/agentLoop.ts`).
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Number of times the model may emit a response that violates the expected
    /// protocol (e.g. malformed tool calls) before the loop is aborted.
    pub max_protocol_strikes: usize,

    /// Number of consecutive iterations in which the assistant produces identical
    /// text content before the loop is considered stagnant and aborted.
    ///
    /// Frontend constant: `MAX_STAGNATION_STEPS = 5`.
    pub max_stagnation_steps: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 25,
            max_parallel_tools: 5,
            tool_timeout_ms: 30_000,
            context_budget_chars: 180_000,
            max_protocol_strikes: 2,
            max_stagnation_steps: 5,
        }
    }
}

// =============================================================================
// Tool schema
// =============================================================================

/// A tool that the LLM may invoke, expressed as a name + JSON Schema.
///
/// This is the agent domain's own type. Adapter layers are responsible for
/// converting their internal tool representations:
///
/// - MCP adapter: `McpTool → ToolDefinition`
/// - Built-in tools: constructed directly
///
/// The agent domain must **not** reference `McpTool` or any crate-external
/// tool type directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Unique function name exposed to the LLM (e.g. `"filesystem_read_file"`).
    pub name: String,

    /// Human-readable description sent to the LLM to explain when to use the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// JSON Schema object describing the tool's input parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

impl ToolDefinition {
    /// Create a minimal tool definition with only a name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            input_schema: None,
        }
    }

    /// Attach a human-readable description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Attach a JSON Schema for the tool's input parameters.
    #[must_use]
    pub fn with_input_schema(mut self, schema: serde_json::Value) -> Self {
        self.input_schema = Some(schema);
        self
    }
}

// =============================================================================
// Tool call / result
// =============================================================================

/// A tool invocation requested by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier for this call, generated by the LLM.
    ///
    /// Must be echoed back in the corresponding [`ToolResult::tool_call_id`].
    pub id: String,

    /// Name of the tool to invoke (matches [`ToolDefinition::name`]).
    pub name: String,

    /// Arguments as a JSON object, matching the tool's `input_schema`.
    pub arguments: serde_json::Value,
}

/// The outcome of executing a [`ToolCall`].
///
/// # Important: failures are context, not errors
///
/// A `ToolResult` with `success: false` is **not** a Rust-level error —
/// it is domain data that gets appended to the conversation as an
/// `AgentMessage::Tool` entry so the model can observe the failure and
/// adjust its strategy (retry, fix arguments, use a different tool, etc.).
///
/// Infrastructure-level failures (e.g. the MCP process crashed) are
/// surfaced separately via `anyhow::Error` from [`ToolExecutorPort::execute`]
/// and then converted by the loop implementation into a `ToolResult` with
/// `success: false` and an appropriate `content` describing the outage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Echoes the [`ToolCall::id`] this result corresponds to.
    pub tool_call_id: String,

    /// Human-readable output (or error message) from the tool execution.
    pub content: String,

    /// Whether the tool completed successfully.
    ///
    /// `false` here is **not** a loop error — see type-level docs above.
    pub success: bool,

    /// Wall-clock time taken to execute the tool, in milliseconds.
    pub duration_ms: u64,
}

// =============================================================================
// Conversation messages
// =============================================================================

/// A single message in the agent conversation.
///
/// The closed enum prevents invalid states that a flat struct with `role: String`
/// would allow (e.g. a `User` message carrying `tool_calls`, or a `Tool` message
/// without a `tool_call_id`).
///
/// # Wire format
///
/// `#[serde(tag = "role", rename_all = "lowercase")]` produces JSON identical to
/// the TypeScript `ChatMessage` interface in the frontend:
///
/// ```json
/// { "role": "user", "content": "What files are in the project?" }
/// { "role": "assistant", "content": null, "tool_calls": [...] }
/// { "role": "tool", "tool_call_id": "call_abc", "content": "src/\nlib/" }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum AgentMessage {
    /// A system-level instruction that sets the model's persona and constraints.
    System {
        /// Instruction text.
        content: String,
    },

    /// A message from the human user.
    User {
        /// Message text.
        content: String,
    },

    /// A response from the assistant model.
    ///
    /// Either `content` **or** `tool_calls` is non-`None`; both may be present
    /// when the model produces a reasoning preamble before requesting tool calls.
    Assistant {
        /// Optional text content of the response.
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,

        /// Tool calls requested by the model (triggers the tool execution phase).
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCall>>,
    },

    /// The result of a tool call, to be sent back to the model.
    Tool {
        /// Must match the [`ToolCall::id`] from the preceding `Assistant` message.
        tool_call_id: String,

        /// Serialised output of the tool (or error description if it failed).
        content: String,
    },
}

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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// The model has produced a reasoning / thinking segment.
    Thinking {
        /// The reasoning text (may be partial if streamed).
        content: String,
    },

    /// An incremental text fragment from the model's response.
    TextDelta {
        /// The new text fragment (append to the current buffer).
        content: String,
    },

    /// The model has requested execution of a tool.
    ToolCallStart {
        /// The tool call that is about to be dispatched.
        tool_call: ToolCall,
    },

    /// A tool execution has completed (success or failure).
    ToolCallComplete {
        /// The outcome of the tool, including timing and success flag.
        result: ToolResult,
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
#[derive(Debug, Clone)]
pub enum LlmStreamEvent {
    /// An incremental text fragment from the model's response.
    ///
    /// Must be forwarded immediately to the caller as an
    /// [`AgentEvent::TextDelta`] to preserve real-time UX.
    TextDelta {
        /// The new text fragment (append to the running content buffer).
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
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_config_defaults_match_frontend_constants() {
        let cfg = AgentConfig::default();
        assert_eq!(cfg.max_iterations, 25);
        assert_eq!(cfg.max_parallel_tools, 5);
        assert_eq!(cfg.tool_timeout_ms, 30_000);
        assert_eq!(cfg.context_budget_chars, 180_000);
        assert_eq!(cfg.max_protocol_strikes, 2);
        assert_eq!(
            cfg.max_stagnation_steps, 5,
            "must mirror MAX_STAGNATION_STEPS from agentLoop.ts"
        );
    }

    #[test]
    fn tool_definition_builder_sets_fields() {
        let tool = ToolDefinition::new("search")
            .with_description("Full-text search")
            .with_input_schema(json!({ "type": "object" }));
        assert_eq!(tool.name, "search");
        assert!(tool.description.is_some());
        assert!(tool.input_schema.is_some());
    }

    #[test]
    fn agent_message_serde_tag_matches_wire_format() {
        let msg = AgentMessage::Tool {
            tool_call_id: "call_1".into(),
            content: "ok".into(),
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "tool");
        assert_eq!(json["tool_call_id"], "call_1");
    }

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
    fn tool_result_success_false_is_serialisable() {
        let result = ToolResult {
            tool_call_id: "c1".into(),
            content: "ERROR: file not found".into(),
            success: false,
            duration_ms: 12,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["success"], false);
    }
}
