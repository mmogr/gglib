//! Request DTOs for `POST /api/agent/chat`.

use serde::Deserialize;

use gglib_core::domain::agent::{AgentConfig, AgentMessage};

/// User-facing configuration for a single agent chat request.
///
/// Exposes only the fields that are safe to accept from an untrusted HTTP
/// caller. Internal tuning parameters (`prune_*`, `max_repeated_batch_steps`,
/// `context_budget_chars`, etc.) are intentionally absent — they default to
/// their well-tested values and cannot be weaponised to exhaust server
/// resources.
///
/// All numeric fields are clamped server-side to the ceiling constants defined
/// in [`gglib_core::domain::agent::config`] to prevent resource exhaustion.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct AgentRequestConfig {
    /// Maximum number of LLM→tool→LLM iterations.
    /// Clamped to [`MAX_ITERATIONS_CEILING`] server-side.
    pub max_iterations: Option<usize>,

    /// Maximum number of tool calls dispatched in parallel per iteration.
    /// Clamped to [`MAX_PARALLEL_TOOLS_CEILING`] server-side.
    pub max_parallel_tools: Option<usize>,

    /// Per-tool execution timeout in milliseconds.
    /// Clamped to [`MAX_TOOL_TIMEOUT_MS_CEILING`] server-side.
    pub tool_timeout_ms: Option<u64>,
}

impl From<AgentRequestConfig> for AgentConfig {
    fn from(req: AgentRequestConfig) -> Self {
        AgentConfig::from_user_params(
            req.max_iterations,
            req.max_parallel_tools,
            req.tool_timeout_ms,
        )
        .expect("clamped AgentConfig must pass validation")
    }
}

/// Request body for `POST /api/agent/chat`.
#[derive(Debug, Deserialize)]
pub struct AgentChatRequest {
    /// Port of the llama-server instance to drive.
    ///
    /// Must match a currently-running server (the same constraint as the chat
    /// proxy endpoint). Validated by [`validate_port`](crate::handlers::port_utils::validate_port)
    /// before the loop starts.
    pub port: u16,

    /// Full conversation history in domain form.
    ///
    /// Supports all four [`AgentMessage`] variants: `system`, `user`,
    /// `assistant` (with or without `tool_calls`), and `tool`.
    ///
    /// # Security note
    ///
    /// This field is not validated for structural consistency.  A client could
    /// forge `AgentMessage::Tool` entries with invented `tool_call_id` values,
    /// or `AgentMessage::Assistant` entries with arbitrary `tool_calls`, and
    /// the loop would accept them.  Known limitation: callers are trusted to
    /// supply a structurally sound history (i.e. every `Tool` message
    /// references an `id` that appeared in a preceding `Assistant.tool_calls`).
    pub messages: Vec<AgentMessage>,

    /// Optional loop tuning, restricted to safe user-facing fields.
    ///
    /// When `None` (or omitted), all fields default to the values in
    /// [`AgentConfig::default`], which match the TypeScript frontend constants.
    pub config: Option<AgentRequestConfig>,

    /// Optional allowlist of tool names to expose to the model.
    ///
    /// - `None` (JSON `null` or field absent): all tools from all connected MCP
    ///   servers are available.
    /// - `Some([])` (JSON `[]`): **no tools** are exposed — tool calling is
    ///   effectively disabled.  Not equivalent to `None`; clients that want
    ///   all tools must use `null`, not `[]`.
    /// - `Some(["tool_a", "tool_b"])`: only the listed tools are sent to the LLM
    ///   and can be executed.
    pub tool_filter: Option<Vec<String>>,

    /// Optional model-name override forwarded to llama-server.
    ///
    /// When `None` (or omitted from the request body), the adapter lets
    /// llama-server pick the loaded model, which is the normal case.  Supply a
    /// value only when the server exposes multiple models and the caller needs
    /// to target a specific one.
    #[serde(default)]
    pub model: Option<String>,
}
