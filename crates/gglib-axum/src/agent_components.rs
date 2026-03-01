//! Shared factory for composing the agentic loop from its infrastructure parts.
//!
//! Both the HTTP handler (`POST /api/agent/chat`) and the CLI `agent-chat`
//! command need to wire the same three pieces together:
//!
//! ```text
//! LlmCompletionAdapter ─┐
//!                        ├─► AgentLoop::build ─► Arc<dyn AgentLoopPort>
//! McpToolExecutorAdapter ┘
//! ```
//!
//! Centralising this in [`AgentComponents::build`] eliminates the duplicated
//! composition boilerplate that would otherwise live in both call sites.

use std::collections::HashSet;
use std::sync::Arc;

use gglib_agent::AgentLoop;
use gglib_core::ports::AgentLoopPort;
use gglib_mcp::{McpService, McpToolExecutorAdapter};
use gglib_runtime::LlmCompletionAdapter;
use reqwest::Client;

/// Stateless factory for assembling a ready-to-use [`AgentLoopPort`].
///
/// The struct carries no state — all parameters are passed to [`build`].
///
/// [`build`]: AgentComponents::build
pub struct AgentComponents;

impl AgentComponents {
    /// Wire `LlmCompletionAdapter + McpToolExecutorAdapter + AgentLoop::build`.
    ///
    /// # Parameters
    ///
    /// - `port` — TCP port of the llama-server instance to target.
    /// - `client` — reqwest client to reuse.  HTTP callers should pass
    ///   `state.http_client.clone()`; CLI callers should pass `Client::new()`.
    /// - `mcp` — shared MCP service handle.
    /// - `tool_filter` — optional allowlist of tool names.  `None` exposes all
    ///   tools registered with the MCP service.
    pub fn build(
        port: u16,
        client: Client,
        mcp: Arc<McpService>,
        tool_filter: Option<HashSet<String>>,
    ) -> Arc<dyn AgentLoopPort> {
        let llm = Arc::new(LlmCompletionAdapter::with_client(
            port,
            client,
            None::<String>,
        ));
        let mcp_executor = Arc::new(McpToolExecutorAdapter::new(mcp));
        AgentLoop::build(llm, mcp_executor, tool_filter)
    }
}
