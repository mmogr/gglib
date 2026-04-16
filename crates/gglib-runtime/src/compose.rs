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
use std::path::PathBuf;
use std::sync::Arc;

use gglib_agent::AgentLoop;
use gglib_core::domain::InferenceConfig;
use gglib_core::ports::{AgentLoopPort, LlmCompletionPort, ToolExecutorPort};
use gglib_mcp::{CombinedToolExecutor, McpService};
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
    compose_agent_loop_inner(base_url, http_client, model, mcp, tool_filter, None, None)
}

/// Like [`compose_agent_loop`] but with filesystem tools sandboxed to `sandbox_root`.
pub fn compose_agent_loop_sandboxed(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    mcp: Arc<McpService>,
    tool_filter: Option<HashSet<String>>,
    sandbox_root: PathBuf,
) -> Arc<dyn AgentLoopPort> {
    compose_agent_loop_inner(
        base_url,
        http_client,
        model,
        mcp,
        tool_filter,
        Some(sandbox_root),
        None,
    )
}

/// Like [`compose_agent_loop`] with optional sampling overrides and sandbox.
pub fn compose_agent_loop_with_sampling(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    mcp: Arc<McpService>,
    tool_filter: Option<HashSet<String>>,
    sandbox_root: Option<PathBuf>,
    sampling: Option<InferenceConfig>,
) -> Arc<dyn AgentLoopPort> {
    compose_agent_loop_inner(
        base_url,
        http_client,
        model,
        mcp,
        tool_filter,
        sandbox_root,
        sampling,
    )
}

/// Return type for [`compose_council_ports`].
pub struct CouncilPorts {
    /// LLM completion port shared across all agent turns in the council.
    pub llm: Arc<dyn LlmCompletionPort>,
    /// Tool executor shared across all agent turns in the council.
    pub tool_executor: Arc<dyn ToolExecutorPort>,
}

/// Compose the raw infrastructure ports needed by the council orchestrator.
///
/// Unlike [`compose_agent_loop`] — which returns a fully-assembled
/// [`AgentLoopPort`] — this returns the underlying [`LlmCompletionPort`] and
/// [`ToolExecutorPort`] separately, because
/// [`gglib_agent::council::run_council`] creates per-agent `AgentLoop`
/// instances internally (each with its own tool filter).
pub fn compose_council_ports(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    mcp: Arc<McpService>,
) -> CouncilPorts {
    let llm: Arc<dyn LlmCompletionPort> = Arc::new(LlmCompletionAdapter::with_client(
        base_url,
        http_client,
        model,
    ));
    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(CombinedToolExecutor::new(mcp));
    CouncilPorts { llm, tool_executor }
}

fn compose_agent_loop_inner(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    mcp: Arc<McpService>,
    tool_filter: Option<HashSet<String>>,
    sandbox_root: Option<PathBuf>,
    sampling: Option<InferenceConfig>,
) -> Arc<dyn AgentLoopPort> {
    let llm: Arc<dyn LlmCompletionPort> = Arc::new(
        LlmCompletionAdapter::with_client(base_url, http_client, model).with_sampling(sampling),
    );
    let tool_executor: Arc<dyn ToolExecutorPort> = match sandbox_root {
        Some(root) => Arc::new(CombinedToolExecutor::with_sandbox(mcp, root)),
        None => Arc::new(CombinedToolExecutor::new(mcp)),
    };
    AgentLoop::build(llm, tool_executor, tool_filter)
}
