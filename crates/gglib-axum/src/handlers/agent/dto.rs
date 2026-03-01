//! Request DTOs for `POST /api/agent/chat`.

use serde::Deserialize;

use gglib_core::domain::agent::{AgentConfig, AgentMessage};
use gglib_core::{MAX_ITERATIONS_CEILING, MAX_PARALLEL_TOOLS_CEILING, MAX_TOOL_TIMEOUT_MS_CEILING};

/// User-facing configuration for a single agent chat request.
///
/// Exposes only the fields that are safe to accept from an untrusted HTTP
/// caller. Internal tuning parameters (`prune_*`, `max_empty_tool_response_steps`,
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
        let AgentRequestConfig { max_iterations, max_parallel_tools, tool_timeout_ms } = req;
        let mut cfg = AgentConfig::default();
        if let Some(n) = max_iterations {
            cfg.max_iterations = n.min(MAX_ITERATIONS_CEILING);
        }
        if let Some(n) = max_parallel_tools {
            cfg.max_parallel_tools = n.min(MAX_PARALLEL_TOOLS_CEILING);
        }
        if let Some(ms) = tool_timeout_ms {
            cfg.tool_timeout_ms = ms.min(MAX_TOOL_TIMEOUT_MS_CEILING);
        }
        cfg
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
    pub messages: Vec<AgentMessage>,

    /// Optional loop tuning, restricted to safe user-facing fields.
    ///
    /// When `None` (or omitted), all fields default to the values in
    /// [`AgentConfig::default`], which match the TypeScript frontend constants.
    pub config: Option<AgentRequestConfig>,

    /// Optional allowlist of tool names to expose to the model.
    ///
    /// When `Some`, only tools whose names appear in this list are sent to the
    /// LLM and can be executed. When `None`, all tools from all connected MCP
    /// servers are available.
    pub tool_filter: Option<Vec<String>>,
}
