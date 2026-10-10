//! Shared agent-loop composition root.
//!
//! Both the HTTP handler (`gglib-axum`) and the CLI (`gglib-cli`) need the
//! same three-step wiring sequence:
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
    AgentGuardReporter, AgentLoopPort, AttachmentStore, GenerationGate, LlmCompletionPort,
    RetryObserver, ToolExecutorPort, UsageSink,
};
use gglib_core::request_pipeline::{ModelContext, SamplingLayers};
use gglib_core::retry::RetryPolicy;
use gglib_core::services::AttachmentService;
use gglib_mcp::{
    BuiltinToolExecutorAdapter, CombinedToolExecutor, DrawArm, DrawingTool, McpService,
};
use reqwest::Client;

use crate::{FarMachine, LlmCompletionAdapter, SamplingObserver};

/// What a loop needs to share the GPU with image generation.
#[derive(Debug, Clone, Default)]
pub struct LoopGeneration {
    /// The gate each send to this machine's llama-server waits on for its
    /// turn, holding it until the reply ends
    /// ([`LlmCompletionAdapter::with_generation_gate`]). `None` waits on
    /// nothing: a session with no daemon to share the GPU with.
    pub gate: Option<Arc<dyn GenerationGate>>,
    /// The drawing tool and when it is offered: armed for a run sent with
    /// `draw: true`, or a session's switch that `/draw` sets. `None` never
    /// offers `generate_image`, whatever the tool filter says.
    pub drawing: Option<(DrawingTool, DrawArm)>,
}

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
/// * `guard` — where the loop reports every decision its guard takes, and the
///   model name to count those decisions under (#1091). Not an `Option`, and
///   deliberately: this function has one caller, `POST /api/agent/chat`, which
///   runs in the same process as the embedded proxy and can always reach its
///   ledger, so "the wiring reports nothing" is a mistake the compiler can
///   refuse rather than one a test has to catch. A caller that genuinely has
///   nowhere to report — the CLI, which runs out of process — wants
///   [`compose_agent_loop_with_sampling`], whose own parameter is optional.
/// * `retry_observer` — `Some(observer)` surfaces upstream retries to a live
///   consumer, so a user waiting on a contended model is told why. `None` when
///   there is no stream to notify.
/// * `sampling` — what a person chose for this turn, the ladder's top rung,
///   or `None` to resolve entirely from `layers`, the model's own defaults and
///   the floor. `POST /api/agent/chat` passes the request's reasoning controls
///   and nothing else; see `AgentChatRequest::sampling_layer` for why that pair
///   and not the sampler parameters.
/// * `layers` — the stored layers beneath `sampling`, handed over unfolded:
///   the adapter gives them to [`gglib_core::request_pipeline::apply()`], the
///   one place the ladder is folded. A run passes the settings' global
///   defaults and no profile, since its request can name none. With them
///   goes whether a turn with tools gets the agentic temperature ceiling:
///   the settings' switch for a run on this machine's model, and on for one
///   on a paired machine's.
/// * `far_machine` — `Some(machine)` when `base_url` is the remote tunnel's
///   loopback port, which is another machine's proxy (ADR 0012): it carries
///   both the key that port demands, since the listener there injects none,
///   and the fingerprint that names whose port it is, so a refusal of that key
///   can say which machine refused it. `None` for a llama-server on loopback,
///   which demands nothing and is not another machine.
/// * `attachments` — where the images a message names by id are read from,
///   just before each request is sent, and where the images an MCP tool
///   returns are stored.
/// * `generation` — the daemon's generation gate, so a reply on this
///   machine's model waits for an image render rather than sharing the GPU
///   with it. A send to `far_machine` takes no turn. With it, the drawing
///   tool, offered only while its arm says so.
#[allow(clippy::too_many_arguments)]
#[allow(
    clippy::implicit_hasher,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub fn compose_agent_loop(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    model_context: ModelContext,
    mcp: Arc<McpService>,
    tool_filter: Option<HashSet<String>>,
    usage_sink: Option<Arc<dyn UsageSink>>,
    guard: AgentGuardReporter,
    retry_observer: Option<Arc<dyn RetryObserver>>,
    sampling: Option<InferenceConfig>,
    layers: SamplingLayers,
    far_machine: Option<FarMachine>,
    attachments: Arc<dyn AttachmentStore>,
    generation: LoopGeneration,
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
        layers,
        // A run's request names no sampler parameter the ladder could pass
        // over, so nobody is waiting to hear how it resolved.
        None,
        usage_sink,
        Some(guard),
        retry_observer,
        // The GUI has no per-turn retry override; the environment defaults apply.
        None,
        far_machine,
        attachments,
        generation,
    )
}

/// Like [`compose_agent_loop`] with optional sampling overrides and sandbox.
///
/// `sampling` is the flags a person typed, and `layers` the profile they
/// selected, the settings' global defaults and the agentic switch, as
/// [`compose_agent_loop`] takes them. `sampling_observer` is told what each
/// request's sampling resolved to, which is how the terminal learns of a flag
/// the ladder passed over without folding a ladder of its own.
///
/// `retry_policy` bounds retrying of transient upstream failures; pass `None`
/// to use the defaults with any `GGLIB_LLM_RETRY_*` overrides applied.
///
/// `guard` is optional here where [`compose_agent_loop`]'s is not: this is the
/// CLI's entry point, and `gglib chat` runs out of process, so the ledger the
/// GUI reports to is not something it can reach (#1091). `None` makes every
/// recording a no-op.
///
/// `attachments` is the store the CLI's own images were put in: the CLI
/// opens the database itself.
///
/// `generation` carries the daemon's gate when one is running to share the
/// GPU with; `gglib chat` takes its turns there over a connection.
#[allow(clippy::too_many_arguments)]
#[allow(
    clippy::implicit_hasher,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub fn compose_agent_loop_with_sampling(
    base_url: String,
    http_client: Client,
    model: Option<String>,
    model_context: ModelContext,
    mcp: Arc<McpService>,
    tool_filter: Option<HashSet<String>>,
    sandbox_root: Option<PathBuf>,
    sampling: Option<InferenceConfig>,
    layers: SamplingLayers,
    sampling_observer: Option<SamplingObserver>,
    usage_sink: Option<Arc<dyn UsageSink>>,
    guard: Option<AgentGuardReporter>,
    retry_policy: Option<RetryPolicy>,
    far_machine: Option<FarMachine>,
    attachments: Arc<dyn AttachmentStore>,
    generation: LoopGeneration,
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
        layers,
        sampling_observer,
        usage_sink,
        guard,
        // The CLI renders the loop's events directly, so there is no separate
        // consumer to notify — retries surface through the loop's own output.
        None,
        retry_policy,
        far_machine,
        attachments,
        generation,
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
    layers: SamplingLayers,
    sampling_observer: Option<SamplingObserver>,
    usage_sink: Option<Arc<dyn UsageSink>>,
    guard: Option<AgentGuardReporter>,
    retry_observer: Option<Arc<dyn RetryObserver>>,
    retry_policy: Option<RetryPolicy>,
    far_machine: Option<FarMachine>,
    attachments: Arc<dyn AttachmentStore>,
    generation: LoopGeneration,
) -> Arc<dyn AgentLoopPort> {
    let images = Arc::new(AttachmentService::new(Arc::clone(&attachments)));
    let llm: Arc<dyn LlmCompletionPort> = Arc::new(
        LlmCompletionAdapter::with_client(base_url, http_client, model)
            .with_far_machine(far_machine)
            .with_attachments(Some(attachments))
            .with_sampling(sampling)
            .with_layers(layers)
            .with_sampling_observer(sampling_observer)
            .with_model_context(model_context)
            .with_usage_sink(usage_sink)
            .with_retry_observer(retry_observer)
            .with_retry_policy(retry_policy.unwrap_or_else(RetryPolicy::from_env))
            .with_generation_gate(generation.gate),
    );
    let builtin = sandbox_root
        .map_or_else(
            BuiltinToolExecutorAdapter::default,
            BuiltinToolExecutorAdapter::with_sandbox,
        )
        .with_drawing(generation.drawing);
    let tool_executor: Arc<dyn ToolExecutorPort> =
        Arc::new(CombinedToolExecutor::with_builtin(mcp, images, builtin));
    AgentLoop::build_observed(llm, tool_executor, tool_filter, guard)
}
