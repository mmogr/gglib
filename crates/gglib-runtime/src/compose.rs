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
//!
//! Every entry point hands in a [`ModelContext`] resolved by
//! [`gglib_core::request_pipeline::resolve`] rather than a bare tag list, so
//! the agent path carries the same per-model facts the proxy does — and now
//! acts on all of them: capabilities drive request-side message coalescing,
//! inference defaults are a layer of the sampling hierarchy, and `format:*`
//! tags select the response parser. The context is handed to the adapter whole
//! rather than being taken apart here.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use gglib_agent::AgentLoop;
use gglib_core::domain::InferenceConfig;
use gglib_core::ports::{AgentLoopPort, CacheMetricsSink, LlmCompletionPort, ToolExecutorPort};
use gglib_core::request_pipeline::ModelContext;
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
/// * `model_context` — resolved per-model facts from
///   [`gglib_core::request_pipeline::resolve`], driving both request shaping
///   and response-parser selection. Pass [`ModelContext::passthrough`] when the
///   model is unknown: every transform becomes a no-op and the identity parser
///   is selected.
/// * `mcp` — handle to the running MCP service (for tool discovery/execution).
/// * `tool_filter` — `Some(set)` restricts the visible tools to the named
///   allowlist; `None` exposes all tools from all connected MCP servers.
/// * `cache_metrics` — `Some(sink)` reports this loop's prompt-cache reuse
///   (e.g. the proxy process's agent-path store, for GUI chat); `None` when
///   there is no dashboard to report to.
pub fn compose_agent_loop(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    model_context: ModelContext,
    mcp: Arc<McpService>,
    tool_filter: Option<HashSet<String>>,
    cache_metrics: Option<Arc<dyn CacheMetricsSink>>,
) -> Arc<dyn AgentLoopPort> {
    compose_agent_loop_inner(
        base_url,
        http_client,
        model,
        model_context,
        mcp,
        tool_filter,
        None,
        None,
        cache_metrics,
    )
}

/// Like [`compose_agent_loop`] with optional sampling overrides and sandbox.
#[allow(clippy::too_many_arguments)]
pub fn compose_agent_loop_with_sampling(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    model_context: ModelContext,
    mcp: Arc<McpService>,
    tool_filter: Option<HashSet<String>>,
    sandbox_root: Option<PathBuf>,
    sampling: Option<InferenceConfig>,
    cache_metrics: Option<Arc<dyn CacheMetricsSink>>,
) -> Arc<dyn AgentLoopPort> {
    compose_agent_loop_inner(
        base_url,
        http_client,
        model,
        model_context,
        mcp,
        tool_filter,
        sandbox_root,
        sampling,
        cache_metrics,
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
///
/// `sampling` applies to every LLM call made during the council run (planning,
/// worker turns, synthesis, compaction).  Pass `None` to use llama-server
/// defaults.
///
/// When `sandbox_root` is `Some`, filesystem tools (`read_file`,
/// `list_directory`, `grep_search`) are enabled and scoped to that path.
///
/// `cache_metrics` reports every LLM call's prompt-cache reuse to a sink when
/// the council runs in the proxy process; `None` when there is no dashboard.
#[allow(clippy::too_many_arguments)]
pub fn compose_council_ports(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    model_context: ModelContext,
    mcp: Arc<McpService>,
    sandbox_root: Option<PathBuf>,
    sampling: Option<InferenceConfig>,
    cache_metrics: Option<Arc<dyn CacheMetricsSink>>,
) -> CouncilPorts {
    let llm: Arc<dyn LlmCompletionPort> = Arc::new(
        LlmCompletionAdapter::with_client(base_url, http_client, model)
            .with_sampling(sampling)
            .with_model_context(model_context)
            .with_cache_metrics_sink(cache_metrics),
    );
    let tool_executor: Arc<dyn ToolExecutorPort> = match sandbox_root {
        Some(root) => Arc::new(CombinedToolExecutor::with_sandbox(mcp, root)),
        None => Arc::new(CombinedToolExecutor::new(mcp)),
    };
    CouncilPorts { llm, tool_executor }
}

#[allow(clippy::too_many_arguments)]
fn compose_agent_loop_inner(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    model_context: ModelContext,
    mcp: Arc<McpService>,
    tool_filter: Option<HashSet<String>>,
    sandbox_root: Option<PathBuf>,
    sampling: Option<InferenceConfig>,
    cache_metrics: Option<Arc<dyn CacheMetricsSink>>,
) -> Arc<dyn AgentLoopPort> {
    let llm: Arc<dyn LlmCompletionPort> = Arc::new(
        LlmCompletionAdapter::with_client(base_url, http_client, model)
            .with_sampling(sampling)
            .with_model_context(model_context)
            .with_cache_metrics_sink(cache_metrics),
    );
    let tool_executor: Arc<dyn ToolExecutorPort> = match sandbox_root {
        Some(root) => Arc::new(CombinedToolExecutor::with_sandbox(mcp, root)),
        None => Arc::new(CombinedToolExecutor::new(mcp)),
    };
    AgentLoop::build(llm, tool_executor, tool_filter)
}
