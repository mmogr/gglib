//! Shared agent-loop composition root.
//!
//! The daemon's agent runs (`gglib-axum`) are wired in three steps:
//!
//! 1. `LlmCompletionAdapter::with_client(…)` — wrap `reqwest::Client` as an
//!    [`LlmCompletionPort`].
//! 2. `CombinedToolExecutor::{new, with_sandbox}(…)` — wrap [`McpService`] as a
//!    [`ToolExecutorPort`], routing qualified names to MCP and bare ones to the
//!    built-ins, and storing an MCP tool's images in the same attachment store
//!    the completion adapter reads.
//! 3. `AgentLoop::build_observed(llm, tool_executor, tool_filter, guard)` —
//!    compose both ports into an [`AgentLoopPort`], optionally filtering the
//!    tool set, and say where the loop's guard decisions are counted.
//!
//! One function, so every run applies the same defaults and wiring order.
//! One of those defaults is a turn's limit: a request that names no `max_tokens`
//! is limited to 16384 tokens, 32768 for a `reasoning` model, or more under
//! a thinking budget (`LlmCompletionAdapter::with_turn_limit`), so a turn
//! the model never ends stops. A send to another machine is not limited.
//!
//! Every run hands in a [`ModelContext`] resolved by
//! [`gglib_core::request_pipeline::resolve()`] rather than a bare tag list.
//! Through an endpoint the request is shaped there, so the context decides
//! only what the adapter still does itself: whether the most recent drawn
//! picture is shown, and a turn's limit. The context is handed to the
//! adapter whole rather than being taken apart here.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use gglib_agent::AgentLoop;
use gglib_core::domain::InferenceConfig;
use gglib_core::ports::{
    AgentGuardReporter, AgentLoopPort, AttachmentStore, LlmCompletionPort, RetryObserver,
    ToolExecutorPort,
};
use gglib_core::request_pipeline::{ModelContext, SamplingLayers};
use gglib_core::retry::RetryPolicy;
use gglib_core::services::AttachmentService;
use gglib_mcp::{
    BuiltinToolExecutorAdapter, CombinedToolExecutor, DrawArm, DrawingTool, McpService,
};
use reqwest::Client;

use crate::{Door, LlmCompletionAdapter};

/// Compose a ready-to-run [`AgentLoopPort`] from infrastructure primitives.
///
/// # Parameters
///
/// * `base_url` — the root the loop posts to: this machine's endpoint (its
///   `door`'s `root_url`), or a paired machine's through the tunnel.
/// * `http_client` — shared `reqwest::Client` (connection-pooled).
/// * `model` — the model the body names: its id at the endpoint, with any
///   profile as its `:<profile>` suffix.
/// * `model_context` — resolved per-model facts from
///   [`gglib_core::request_pipeline::resolve()`]. Pass
///   [`ModelContext::passthrough`] when the model is unknown.
/// * `mcp` — handle to the running MCP service (for tool discovery/execution).
/// * `tool_filter` — `Some(set)` restricts the visible tools to the named
///   allowlist; `None` exposes all tools from all connected MCP servers.
/// * `sandbox_root` — the run's folder, a canonical path, where gglib's own
///   file tools read: `Some(folder)` offers them with descriptions that name
///   it, and `None` leaves them out. External MCP servers are told nothing
///   of it (ADR 0023).
/// * `guard` — where the loop reports every decision its guard takes, and the
///   model name to count those decisions under (#1091). `None` for a loop
///   whose turns go through this machine's endpoint: the endpoint's own loop
///   guard sees every request and counts them, and a second count here would
///   count each trip twice.
/// * `retry_observer` — `Some(observer)` surfaces upstream retries to a live
///   consumer, so a user waiting on a contended model is told why. `None` when
///   there is no stream to notify.
/// * `sampling` — what the turn names, the ladder's top rung, or `None` for
///   nothing: its thinking budget and its chat's saved sampling, sent as a
///   client's.
/// * `layers` — the stored layers the endpoint folds beneath `sampling`,
///   read here for one thing: whether its ladder names a `max_tokens`, which
///   a turn's limit would otherwise outrank.
/// * `door` — the gglib endpoint `base_url` is: this machine's own, which a
///   chat turn dials as any client does, or the remote tunnel's loopback
///   port, which is another machine's proxy (ADR 0012). It carries the key
///   the endpoint demands and, for another machine, the name of whose port
///   it is, so a refusal of that key can say which machine refused it. A
///   send through it is proxied: the endpoint shapes it, parses the model's
///   dialect and admits it on its queue.
/// * `attachments` — where the images a message names by id are read from,
///   just before each request is sent, and where the images an MCP tool
///   returns are stored.
/// * `drawing` — the drawing tool and when it is offered: armed for a run
///   sent with `draw: true`. `None` never offers `generate_image`, whatever
///   the tool filter says.
#[expect(
    clippy::implicit_hasher,
    reason = "every caller passes the default-hashed set its tool filter is built as; a hasher parameter would add a generic and nothing else"
)]
#[allow(clippy::too_many_arguments)]
pub fn compose_agent_loop(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    model_context: ModelContext,
    mcp: Arc<McpService>,
    tool_filter: Option<HashSet<String>>,
    sandbox_root: Option<PathBuf>,
    guard: Option<AgentGuardReporter>,
    retry_observer: Option<Arc<dyn RetryObserver>>,
    sampling: Option<InferenceConfig>,
    layers: SamplingLayers,
    door: Door,
    attachments: Arc<dyn AttachmentStore>,
    drawing: Option<(DrawingTool, DrawArm)>,
) -> Arc<dyn AgentLoopPort> {
    let images = Arc::new(AttachmentService::new(Arc::clone(&attachments)));
    let llm: Arc<dyn LlmCompletionPort> = Arc::new(
        LlmCompletionAdapter::with_client(base_url, http_client, model)
            .with_door(Some(door))
            .with_attachments(Some(attachments))
            .with_sampling(sampling)
            .with_layers(layers)
            .with_model_context(model_context)
            .with_retry_observer(retry_observer)
            // The run has no per-turn retry override; the environment
            // defaults apply.
            .with_retry_policy(RetryPolicy::from_env())
            .with_turn_limit(true),
    );
    let builtin = sandbox_root
        .map_or_else(
            BuiltinToolExecutorAdapter::default,
            BuiltinToolExecutorAdapter::with_sandbox,
        )
        .with_drawing(drawing);
    let tool_executor: Arc<dyn ToolExecutorPort> =
        Arc::new(CombinedToolExecutor::with_builtin(mcp, images, builtin));
    AgentLoop::build_observed(llm, tool_executor, tool_filter, guard)
}
