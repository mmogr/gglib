//! Shared agent-loop composition root.
//!
//! Both the HTTP handler (`gglib-axum`) and the CLI (`gglib-cli`) need the
//! same three-step wiring sequence:
//!
//! 1. `LlmCompletionAdapter::with_client(…)` — wrap `reqwest::Client` as an
//!    [`LlmCompletionPort`].
//! 2. `CombinedToolExecutor::{new, with_sandbox}(…)` — wrap [`McpService`] as a
//!    [`ToolExecutorPort`], routing qualified names to MCP and bare ones to the
//!    built-ins.
//! 3. `AgentLoop::build(llm, tool_executor, tool_filter)` — compose both
//!    ports into an [`AgentLoopPort`], optionally filtering the tool set.
//!
//! Centralising this into a single function eliminates the copy-paste and
//! ensures both entry points apply the same defaults and wiring order.
//!
//! Every entry point hands in a [`ModelContext`] resolved by
//! [`gglib_core::request_pipeline::resolve()`] rather than a bare tag list, so
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
use gglib_core::ports::{
    AgentLoopPort, LlmCompletionPort, RetryObserver, ToolExecutorPort, UsageSink,
};
use gglib_core::request_pipeline::ModelContext;
use gglib_core::retry::RetryPolicy;
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
///   [`gglib_core::request_pipeline::resolve()`], driving both request shaping
///   and response-parser selection. Pass [`ModelContext::passthrough`] when the
///   model is unknown: every transform becomes a no-op and the identity parser
///   is selected.
/// * `mcp` — handle to the running MCP service (for tool discovery/execution).
/// * `tool_filter` — `Some(set)` restricts the visible tools to the named
///   allowlist; `None` exposes all tools from all connected MCP servers.
/// * `usage_sink` — `Some(sink)` reports each response's token usage (e.g. the
///   proxy process's agent-path cache store, for GUI chat); `None` when there
///   is nothing to report to.
/// * `retry_observer` — `Some(observer)` surfaces upstream retries to a live
///   consumer, so a user waiting on a contended model is told why. `None` when
///   there is no stream to notify.
/// * `sampling` — the caller's own top-rung sampling layer, or `None` to
///   resolve entirely from the profile, per-model, global and floor layers.
///   `POST /api/agent/chat` passes the request's reasoning controls and nothing
///   else; see `AgentChatRequest::sampling_layer` for why that pair and not the
///   sampler parameters.
/// * `bearer` — `Some(key)` when `base_url` demands one: the remote tunnel's
///   loopback port is another machine's proxy (ADR 0012), and the listener
///   there does not inject credentials. `None` for a llama-server on loopback.
#[allow(clippy::too_many_arguments)]
pub fn compose_agent_loop(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    model_context: ModelContext,
    mcp: Arc<McpService>,
    tool_filter: Option<HashSet<String>>,
    usage_sink: Option<Arc<dyn UsageSink>>,
    retry_observer: Option<Arc<dyn RetryObserver>>,
    sampling: Option<InferenceConfig>,
    bearer: Option<String>,
) -> Arc<dyn AgentLoopPort> {
    compose_agent_loop_inner(
        base_url,
        http_client,
        model,
        model_context,
        mcp,
        tool_filter,
        None,
        sampling,
        usage_sink,
        retry_observer,
        // The GUI has no per-turn retry override; the environment defaults apply.
        None,
        bearer,
    )
}

/// Like [`compose_agent_loop`] with optional sampling overrides and sandbox.
///
/// `retry_policy` bounds retrying of transient upstream failures; pass `None`
/// to use the defaults with any `GGLIB_LLM_RETRY_*` overrides applied.
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
    usage_sink: Option<Arc<dyn UsageSink>>,
    retry_policy: Option<RetryPolicy>,
    bearer: Option<String>,
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
        usage_sink,
        // The CLI renders the loop's events directly, so there is no separate
        // consumer to notify — retries surface through the loop's own output.
        None,
        retry_policy,
        bearer,
    )
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
    usage_sink: Option<Arc<dyn UsageSink>>,
    retry_observer: Option<Arc<dyn RetryObserver>>,
    retry_policy: Option<RetryPolicy>,
    bearer: Option<String>,
) -> Arc<dyn AgentLoopPort> {
    let llm: Arc<dyn LlmCompletionPort> = Arc::new(
        LlmCompletionAdapter::with_client(base_url, http_client, model)
            .with_bearer(bearer)
            .with_sampling(sampling)
            .with_model_context(model_context)
            .with_usage_sink(usage_sink)
            .with_retry_observer(retry_observer)
            .with_retry_policy(retry_policy.unwrap_or_else(RetryPolicy::from_env)),
    );
    let tool_executor: Arc<dyn ToolExecutorPort> = match sandbox_root {
        Some(root) => Arc::new(CombinedToolExecutor::with_sandbox(mcp, root)),
        None => Arc::new(CombinedToolExecutor::new(mcp)),
    };
    AgentLoop::build(llm, tool_executor, tool_filter)
}
