//! Composition root for the agentic loop.
//!
//! Wires infrastructure adapters into an [`AgentLoopPort`]:
//!
//! ```text
//! LlmCompletionAdapter ─┐
//!                        ├─► AgentLoop::build ─► Arc<dyn AgentLoopPort>
//! McpToolExecutorAdapter ┘
//! ```
//!
//! Both the HTTP handler (`POST /api/agent/chat`) and the CLI `agent-chat`
//! command call [`build_agent`] to avoid duplicating this wiring.

use std::collections::HashSet;
use std::sync::Arc;

use gglib_core::ports::{AgentLoopPort, LlmCompletionPort, ToolExecutorPort};
use gglib_mcp::{McpService, McpToolExecutorAdapter};
use gglib_runtime::LlmCompletionAdapter;
use reqwest::Client;

use crate::agent_loop::AgentLoop;

/// Wire `LlmCompletionAdapter + McpToolExecutorAdapter + AgentLoop::build`
/// into a ready-to-use [`AgentLoopPort`].
///
/// # Parameters
///
/// - `port` — TCP port of the llama-server instance to target.
/// - `client` — reqwest client to reuse.  HTTP callers should pass
///   `state.http_client.clone()`; CLI callers should pass `Client::new()`.
/// - `mcp` — shared MCP service handle.
/// - `tool_filter` — optional allowlist of tool names.  `None` exposes all
///   tools registered with the MCP service.
pub fn build_agent(
    port: u16,
    client: Client,
    mcp: Arc<McpService>,
    tool_filter: Option<HashSet<String>>,
) -> Arc<dyn AgentLoopPort> {
    let llm: Arc<dyn LlmCompletionPort> =
        Arc::new(LlmCompletionAdapter::with_client(port, client, None::<String>));
    let tool_executor: Arc<dyn ToolExecutorPort> =
        Arc::new(McpToolExecutorAdapter::new(mcp));
    AgentLoop::build(llm, tool_executor, tool_filter)
}
