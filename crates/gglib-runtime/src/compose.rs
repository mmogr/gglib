//! Shared agent-loop composition root.
//!
//! Both the HTTP handler (`gglib-axum`) and the CLI (`gglib-cli`) need the
//! same three-step wiring sequence:
//!
//! 1. `LlmCompletionAdapter::with_client(…)` — wrap `reqwest::Client` as an
//!    [`LlmCompletionPort`].
//! 2. `McpToolExecutorAdapter::new(…)` — wrap [`McpService`] as a
//!    [`ToolExecutorPort`].
//! 3. `AgentLoop::build(llm, tool_executor, tool_filter)` — compose both
//!    ports into an [`AgentLoopPort`], optionally filtering the tool set.
//!
//! Centralising this into a single function eliminates the copy-paste and
//! ensures both entry points apply the same defaults and wiring order.

use std::collections::HashSet;
use std::sync::Arc;

use gglib_agent::AgentLoop;
use gglib_core::ports::{AgentLoopPort, LlmCompletionPort, ToolExecutorPort};
use gglib_mcp::{McpService, McpToolExecutorAdapter};
use reqwest::Client;

use crate::LlmCompletionAdapter;

/// Compose a ready-to-run [`AgentLoopPort`] from infrastructure primitives.
///
/// # Parameters
///
/// * `base_url` — `http://127.0.0.1:{port}` pointing at the llama-server.
/// * `http_client` — shared `reqwest::Client` (connection-pooled).
/// * `model` — optional model-name override forwarded to llama-server.
/// * `mcp` — handle to the running MCP service (for tool discovery/execution).
/// * `tool_filter` — `Some(set)` restricts the visible tools to the named
///   allowlist; `None` exposes all tools from all connected MCP servers.
pub fn compose_agent_loop(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    mcp: Arc<McpService>,
    tool_filter: Option<HashSet<String>>,
) -> Arc<dyn AgentLoopPort> {
    let llm: Arc<dyn LlmCompletionPort> =
        Arc::new(LlmCompletionAdapter::with_client(base_url, http_client, model));
    let tool_executor: Arc<dyn ToolExecutorPort> =
        Arc::new(McpToolExecutorAdapter::new(mcp));
    AgentLoop::build(llm, tool_executor, tool_filter)
}
