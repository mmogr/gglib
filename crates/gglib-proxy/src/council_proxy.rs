//! Virtual model routing for the OpenAI-compatible proxy.
//!
//! Intercepts requests for three built-in virtual model names and routes them
//! to the local Director/Worker orchestrator rather than forwarding to a
//! llama-server instance:
//!
//! | Model name                       | Behaviour |
//! |----------------------------------|-----------|
//! | `gglib-council`             | Auto mode — `HitlMode::None`, full pipeline, streams events as markdown. |
//! | `gglib-council:interactive` | Interactive mode — pauses at plan gate, embeds sentinel, resumes on next turn. |
//! | `gglib-council:native`      | Returns HTTP 400 directing client to `/api/council/run`. |
//!
//! # Auto-mode SSE format
//!
//! OpenAI `content` chunks are streamed as follows:
//!
//! ```text
//! ## 🧭 Planning
//! … plan table …
//!
//! ## 🔧 Working on: <goal>
//! <worker text tokens>
//! <details><summary>🔧 <tool> …</summary>…</details>
//!
//! ## 📝 Synthesizing
//! <synthesis tokens — the user-facing answer>
//! ```
//!
//! # Interactive-mode sentinel
//!
//! After streaming the plan, an HTML comment is embedded in the final chunk:
//!
//! ```html
//! <!-- gglib-run-id:UUID approval_id:UUID -->
//! ```
//!
//! On the next turn the proxy extracts this sentinel from the last assistant
//! message, parses the user reply for approval intent
//! (`yes` / `no` / `edit: <instructions>`), and resumes the run.

use std::convert::Infallible;
use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures_util::StreamExt as _;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use gglib_core::domain::council::events::{COUNCIL_EVENT_CHANNEL_CAPACITY, CouncilEvent};
use gglib_core::domain::council::run::CouncilRunStatus;
use gglib_core::domain::council::task_graph::{HitlMode, NodeStatus, TaskGraph};
use gglib_core::ports::{CouncilApprovalRegistryPort, CouncilRepositoryPort};

use crate::models::{ChatChunkChoice, ChatCompletionChunk, ChatDelta, ErrorResponse, ModelInfo};

// =============================================================================
// Virtual model constants
// =============================================================================

/// Auto mode: `HitlMode::None`; full pipeline with markdown streaming.
pub const VIRTUAL_MODEL_AUTO: &str = "gglib-council";

/// Interactive mode: pauses at plan gate; sentinel-based resume.
pub const VIRTUAL_MODEL_INTERACTIVE: &str = "gglib-council:interactive";

/// Native mode: returns HTTP 400; instructs client to use `/api/council/run`.
pub const VIRTUAL_MODEL_NATIVE: &str = "gglib-council:native";

/// All virtual model names exported for the `/v1/models` listing.
pub const VIRTUAL_MODELS: &[&str] = &[
    VIRTUAL_MODEL_AUTO,
    VIRTUAL_MODEL_INTERACTIVE,
    VIRTUAL_MODEL_NATIVE,
];

// =============================================================================
// CouncilRunnerPort — injected dependency
// =============================================================================

/// Execution parameters forwarded from the proxy to the orchestrator runner.
pub struct CouncilRunParams {
    /// Human-in-the-loop gate policy.
    pub hitl_mode: HitlMode,
    /// Process-local approval registry (for HITL gate parking).
    pub approval_registry: Option<Arc<dyn CouncilApprovalRegistryPort>>,
    /// Repository for persisting run records.
    pub council_repo: Option<Arc<dyn CouncilRepositoryPort>>,
    /// Explicit run id (used when resuming an existing run).
    pub run_id: Option<String>,
    /// Pre-existing graph override (used when resuming).
    pub graph_override: Option<TaskGraph>,
}

/// Port for executing the orchestrator, injected at proxy startup.
///
/// Implemented by [`gglib_runtime::CouncilRunnerAdapter`].  The proxy
/// does not depend on `gglib-runtime` directly (that would create a circular
/// dependency), so the implementation is wired in by the calling crate.
#[async_trait]
pub trait CouncilRunnerPort: Send + Sync + fmt::Debug {
    /// Execute the orchestrator for `goal`, streaming [`CouncilEvent`]s
    /// to `tx`.
    ///
    /// Watches `cancel`; stops cleanly when the token is cancelled (e.g. on
    /// client disconnect).  Returns `Ok(())` on both normal completion and
    /// clean cancellation.
    async fn run(
        &self,
        goal: &str,
        params: CouncilRunParams,
        tx: mpsc::Sender<CouncilEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()>;
}

// =============================================================================
// CouncilDeps
// =============================================================================

/// Orchestration services injected into the proxy at startup.
///
/// `runner` wraps the orchestrator execution engine (`gglib-agent`'s
/// [`execute`](gglib_agent::council::execute)) together with the
/// [`LlmCompletionPort`](gglib_core::ports::LlmCompletionPort) and
/// [`ToolExecutorPort`](gglib_core::ports::ToolExecutorPort) adapters.
///
/// `approval_registry` and `council_repo` are shared with the Axum
/// `POST /api/council/approve/:id` handler so that interactive-mode runs
/// are visible in `GET /api/council/runs`.
#[derive(Clone)]
pub struct CouncilDeps {
    /// Executes the orchestrator pipeline (planning → workers → synthesis).
    pub runner: Arc<dyn CouncilRunnerPort>,
    /// Resolves HITL approval gates.
    pub approval_registry: Arc<dyn CouncilApprovalRegistryPort>,
    /// Persists and retrieves orchestrator run records.
    pub council_repo: Arc<dyn CouncilRepositoryPort>,
}

// =============================================================================
// Request envelope for virtual-model requests
// =============================================================================

/// Minimal slice of a chat completion request needed by the orchestrator proxy.
///
/// Used to extract goal text and optional conversation history for interactive
/// mode without deserialising the full OpenAI body (which may contain arrays
/// in `content`, custom stop sequences, etc.).
#[derive(Debug, Deserialize)]
pub(crate) struct OrchestratorRequest {
    pub model: String,
    pub messages: Vec<MessageSlice>,
}

/// A single chat message — only `role` and a string-valued `content`.
///
/// Virtual model requests from OpenWebUI always use string content; array
/// content (multimodal) is not supported for orchestrator models.
#[derive(Debug, Deserialize, Clone)]
pub(crate) struct MessageSlice {
    pub role: String,
    #[serde(default)]
    pub content: Option<String>,
}

// =============================================================================
// Main dispatch
// =============================================================================

/// Dispatch an intercepted virtual-model request to the appropriate handler.
///
/// Called from [`crate::server::chat_completions`] after the routing envelope
/// identifies a virtual model name.
pub(crate) async fn handle_virtual_model(
    deps: &CouncilDeps,
    model_name: &str,
    body: &Bytes,
) -> Response {
    match model_name {
        VIRTUAL_MODEL_NATIVE => handle_native_mode(),
        VIRTUAL_MODEL_AUTO => match serde_json::from_slice::<OrchestratorRequest>(body) {
            Ok(req) => handle_auto_mode(deps, req).await,
            Err(e) => (
                StatusCode::BAD_REQUEST,
                axum::Json(ErrorResponse::new(
                    format!("Invalid request body: {e}"),
                    "invalid_request",
                )),
            )
                .into_response(),
        },
        VIRTUAL_MODEL_INTERACTIVE => match serde_json::from_slice::<OrchestratorRequest>(body) {
            Ok(req) => handle_interactive_mode(deps, req).await,
            Err(e) => (
                StatusCode::BAD_REQUEST,
                axum::Json(ErrorResponse::new(
                    format!("Invalid request body: {e}"),
                    "invalid_request",
                )),
            )
                .into_response(),
        },
        _ => (
            StatusCode::NOT_FOUND,
            axum::Json(ErrorResponse::new(
                format!("Unknown virtual model: {model_name}"),
                "model_not_found",
            )),
        )
            .into_response(),
    }
}

// =============================================================================
// Native mode
// =============================================================================

fn handle_native_mode() -> Response {
    (
        StatusCode::BAD_REQUEST,
        axum::Json(ErrorResponse::new(
            "gglib-council:native requires the gglib API; \
             use POST /api/council/run directly.",
            "unsupported_model",
        )),
    )
        .into_response()
}

// =============================================================================
// Auto mode  (HitlMode::None)
// =============================================================================

async fn handle_auto_mode(deps: &CouncilDeps, req: OrchestratorRequest) -> Response {
    let goal = match extract_last_user_message(&req.messages) {
        Some(g) => g,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(ErrorResponse::new(
                    "No user message found in the conversation",
                    "invalid_request",
                )),
            )
                .into_response();
        }
    };

    info!(goal = %goal, "orchestrator proxy: auto mode request");

    let (tx, rx) = mpsc::channel::<CouncilEvent>(COUNCIL_EVENT_CHANNEL_CAPACITY);
    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();

    let run_id = Uuid::new_v4().to_string();
    let params = CouncilRunParams {
        hitl_mode: HitlMode::None,
        approval_registry: Some(Arc::clone(&deps.approval_registry)),
        council_repo: Some(Arc::clone(&deps.council_repo)),
        run_id: Some(run_id.clone()),
        graph_override: None,
    };

    let runner = Arc::clone(&deps.runner);
    tokio::spawn(async move {
        if let Err(e) = runner.run(&goal, params, tx, cancel_for_task).await {
            error!(error = %e, "orchestrator proxy: auto mode run failed");
        }
    });

    build_sse_stream(rx, cancel, req.model, false, None)
}

// =============================================================================
// Interactive mode  (HitlMode::ApprovePlan)
// =============================================================================

async fn handle_interactive_mode(deps: &CouncilDeps, req: OrchestratorRequest) -> Response {
    // --- Check for a sentinel in the previous assistant turn ---
    if let Some((run_id, approval_id)) = extract_sentinel(&req.messages) {
        return resume_interactive_run(deps, req, run_id, approval_id).await;
    }

    // --- First turn: start a new plan-approval run ---
    let goal = match extract_last_user_message(&req.messages) {
        Some(g) => g,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(ErrorResponse::new(
                    "No user message found",
                    "invalid_request",
                )),
            )
                .into_response();
        }
    };

    info!(goal = %goal, "orchestrator proxy: interactive mode first turn");

    let run_id = Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel::<CouncilEvent>(COUNCIL_EVENT_CHANNEL_CAPACITY);
    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();

    let params = CouncilRunParams {
        hitl_mode: HitlMode::ApprovePlan,
        approval_registry: Some(Arc::clone(&deps.approval_registry)),
        council_repo: Some(Arc::clone(&deps.council_repo)),
        run_id: Some(run_id.clone()),
        graph_override: None,
    };

    let runner = Arc::clone(&deps.runner);
    tokio::spawn(async move {
        if let Err(e) = runner.run(&goal, params, tx, cancel_for_task).await {
            // The executor may be cancelled when we abort after AwaitingApproval —
            // that is expected and not an error worth logging.
            debug!(error = %e, "orchestrator proxy: interactive first-turn run ended");
        }
    });

    build_sse_stream(rx, cancel, req.model, true, Some(run_id))
}

async fn resume_interactive_run(
    deps: &CouncilDeps,
    req: OrchestratorRequest,
    run_id: String,
    _approval_id: String,
) -> Response {
    let intent = extract_approval_intent(&req.messages);
    info!(
        run_id = %run_id,
        intent = ?intent,
        "orchestrator proxy: interactive mode resume"
    );

    match intent {
        ApprovalIntent::Reject(_reason) => {
            // Mark the run as failed and return a brief SSE response.
            if let Err(e) = deps
                .council_repo
                .update_run_status(&run_id, CouncilRunStatus::Failed)
                .await
            {
                warn!(run_id = %run_id, error = %e, "failed to mark run as rejected");
            }
            build_static_sse("❌ Plan rejected.", &req.model)
        }
        ApprovalIntent::Approve(edit) => {
            // Load the saved graph from the DB.
            let run = match deps.council_repo.get_run(&run_id).await {
                Ok(Some(r)) => r,
                Ok(None) => {
                    return (
                        StatusCode::NOT_FOUND,
                        axum::Json(ErrorResponse::new(
                            format!("Run '{run_id}' not found — cannot resume"),
                            "not_found",
                        )),
                    )
                        .into_response();
                }
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        axum::Json(ErrorResponse::new(
                            format!("Failed to load run: {e}"),
                            "internal_error",
                        )),
                    )
                        .into_response();
                }
            };

            let graph_json = match run.graph_json {
                Some(j) => j,
                None => {
                    return (
                        StatusCode::BAD_REQUEST,
                        axum::Json(ErrorResponse::new(
                            "Run has no saved graph — cannot resume",
                            "bad_request",
                        )),
                    )
                        .into_response();
                }
            };

            let mut graph: TaskGraph = match serde_json::from_str(&graph_json) {
                Ok(g) => g,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        axum::Json(ErrorResponse::new(
                            format!("Failed to parse saved graph: {e}"),
                            "internal_error",
                        )),
                    )
                        .into_response();
                }
            };

            // If the user provided edit instructions, update the graph goal.
            if let Some(instructions) = edit {
                graph.goal = instructions;
            }

            // Reset all non-Done nodes to Pending so the wave loop picks them up.
            for node in graph.nodes.values_mut() {
                if node.status != NodeStatus::Done {
                    node.status = NodeStatus::Pending;
                }
            }

            let (tx, rx) = mpsc::channel::<CouncilEvent>(COUNCIL_EVENT_CHANNEL_CAPACITY);
            let cancel = CancellationToken::new();
            let cancel_for_task = cancel.clone();

            let params = CouncilRunParams {
                hitl_mode: HitlMode::None, // already approved — run to completion
                approval_registry: Some(Arc::clone(&deps.approval_registry)),
                council_repo: Some(Arc::clone(&deps.council_repo)),
                run_id: Some(run_id.clone()),
                graph_override: Some(graph),
            };

            let runner = Arc::clone(&deps.runner);
            let goal_for_log = run_id.clone();
            tokio::spawn(async move {
                if let Err(e) = runner.run(&goal_for_log, params, tx, cancel_for_task).await {
                    error!(run_id = %goal_for_log, error = %e, "orchestrator proxy: resume run failed");
                }
            });

            build_sse_stream(rx, cancel, req.model, false, None)
        }
    }
}

// =============================================================================
// SSE stream builder
// =============================================================================

/// Build an SSE `Response` that maps [`CouncilEvent`]s to OpenAI chunks.
///
/// `interactive` — when `true`, the stream terminates on [`CouncilEvent::AwaitingApproval`]
/// and embeds a sentinel so the next turn can resume the run.
fn build_sse_stream(
    rx: mpsc::Receiver<CouncilEvent>,
    cancel: CancellationToken,
    model: String,
    interactive: bool,
    run_id_for_sentinel: Option<String>,
) -> Response {
    let stream_id = format!("chatcmpl-{}", Uuid::new_v4().simple());

    let sse_stream = async_stream::stream! {
        let _drop_guard = DropCancels(cancel.clone());
        let mut inner = ReceiverStream::new(rx);
        let mut first_chunk = true;

        while let Some(event) = inner.next().await {
            match &event {
                CouncilEvent::AwaitingApproval { approval_id, kind: _ } if interactive => {
                    // Embed sentinel so the next turn can resume.
                    let rid = run_id_for_sentinel.as_deref().unwrap_or("unknown");
                    let sentinel = format!(
                        "\n\n<!-- gglib-run-id:{rid} approval_id:{approval_id} -->\n\n\
                         *Reply **yes** to approve and continue, or **no** to cancel.*"
                    );
                    yield sse_chunk(&stream_id, &model, &sentinel, None, &mut first_chunk);
                    yield stop_chunk(&stream_id, &model);
                    yield done_event();
                    cancel.cancel();
                    return;
                }
                _ => {
                    if let Some(content) = orchestrator_event_to_content(&event) {
                        yield sse_chunk(&stream_id, &model, &content, None, &mut first_chunk);
                    }
                    if matches!(event, CouncilEvent::CouncilComplete { .. }) {
                        yield stop_chunk(&stream_id, &model);
                        yield done_event();
                        return;
                    }
                    if matches!(event, CouncilEvent::CouncilError { .. }) {
                        yield stop_chunk(&stream_id, &model);
                        yield done_event();
                        return;
                    }
                }
            }
        }
        // Channel closed without CouncilComplete (e.g. cancelled).
        yield stop_chunk(&stream_id, &model);
        yield done_event();
    };

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Build a static single-chunk SSE stream (for rejection messages, etc.).
fn build_static_sse(content: &str, model: &str) -> Response {
    let stream_id = format!("chatcmpl-{}", Uuid::new_v4().simple());
    let model = model.to_string();
    let content = content.to_string();

    let sse_stream = async_stream::stream! {
        let mut first = true;
        yield sse_chunk(&stream_id, &model, &content, None, &mut first);
        yield stop_chunk(&stream_id, &model);
        yield done_event();
    };

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

// =============================================================================
// CouncilEvent → markdown content
// =============================================================================

/// Map a single [`CouncilEvent`] to markdown text for the SSE stream.
///
/// Returns `None` for events that should not produce visible output
/// (e.g. progress pings, internal bookkeeping).
pub(crate) fn orchestrator_event_to_content(event: &CouncilEvent) -> Option<String> {
    match event {
        CouncilEvent::PlanProposed { graph } => {
            let mut buf = String::from("## 🧭 Planning\n\n");
            buf.push_str("| Task | Depends on |\n|------|------------|\n");
            for (id, node) in &graph.nodes {
                let deps = if node.depends_on.is_empty() {
                    "—".to_string()
                } else {
                    node.depends_on
                        .iter()
                        .map(|d| d.0.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                buf.push_str(&format!("| {} | {} |\n", id.0, deps));
            }
            buf.push('\n');
            Some(buf)
        }

        CouncilEvent::NodeStarted { goal, node_id } => Some(format!(
            "\n\n## 🔧 Working on: {goal}\n\n<!-- node:{node_id} -->\n"
        )),

        CouncilEvent::NodeTextDelta { delta, .. } => Some(delta.clone()),

        CouncilEvent::NodeToolCallStart {
            display_name,
            args_summary,
            ..
        } => {
            let args = args_summary.as_deref().unwrap_or("");
            Some(format!(
                "\n<details><summary>🔧 {display_name}…</summary>\n\n```\n{args}\n```\n"
            ))
        }

        CouncilEvent::NodeToolCallComplete {
            display_name: _,
            result,
            duration_display,
            ..
        } => {
            let preview: String = result.content.chars().take(200).collect();
            Some(format!(
                "\n**Result** ({duration_display}):\n```\n{preview}\n```\n</details>\n\n"
            ))
        }

        CouncilEvent::NodeComplete { .. } => Some("\n\n".to_string()),

        CouncilEvent::NodeFailed { node_id, error } => {
            Some(format!("\n\n**❌ Node `{node_id}` failed:** {error}\n\n"))
        }

        CouncilEvent::SynthesisStart => Some("\n\n## 📝 Synthesizing\n\n".to_string()),

        CouncilEvent::SynthesisTextDelta { delta } => Some(delta.clone()),

        CouncilEvent::CouncilError { message } => {
            Some(format!("\n\n**❌ Error:** {message}\n"))
        }

        // Informational / bookkeeping — no visible output.
        CouncilEvent::PlanApproved
        | CouncilEvent::PlanRejected { .. }
        | CouncilEvent::ReplanAttempt { .. }
        | CouncilEvent::AwaitingApproval { .. }
        | CouncilEvent::RunCostEstimate { .. }
        | CouncilEvent::SteeringApplied { .. }
        | CouncilEvent::NodeReasoningDelta { .. }
        | CouncilEvent::NodeProgress { .. }
        | CouncilEvent::NodeSystemWarning { .. }
        | CouncilEvent::NodeCompacting { .. }
        | CouncilEvent::SynthesisProgress { .. }
        | CouncilEvent::SynthesisComplete { .. }
        | CouncilEvent::CouncilComplete { .. }
        | CouncilEvent::TeamStarted { .. }
        | CouncilEvent::TeamSynthesized { .. }
        | CouncilEvent::SubteamSpawned { .. }
        | CouncilEvent::WaveCompleted { .. }
        // ── debate events — forwarded as visible content in Phase N;
        //    suppressed here until DebateNodeBody renders them on the frontend.
        | CouncilEvent::DebateRoundStarted { .. }
        | CouncilEvent::DebateAgentTurnStarted { .. }
        | CouncilEvent::DebateAgentTextDelta { .. }
        | CouncilEvent::DebateAgentReasoningDelta { .. }
        | CouncilEvent::DebateAgentToolCallStart { .. }
        | CouncilEvent::DebateAgentToolCallComplete { .. }
        | CouncilEvent::DebateAgentTurnComplete { .. }
        | CouncilEvent::DebateJudgeStarted { .. }
        | CouncilEvent::DebateJudgeTextDelta { .. }
        | CouncilEvent::DebateJudgeSummary { .. }
        | CouncilEvent::DebateRoundCompacted { .. }
        | CouncilEvent::DebateStanceMap { .. }
        | CouncilEvent::DebateSynthesisStarted { .. }
        | CouncilEvent::DebateSynthesisTextDelta { .. }
        | CouncilEvent::DebateSynthesisComplete { .. } => None,
    }
}

// =============================================================================
// Helper: build individual SSE events
// =============================================================================

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn sse_chunk(
    id: &str,
    model: &str,
    content: &str,
    finish_reason: Option<&str>,
    first_chunk: &mut bool,
) -> Result<Event, Infallible> {
    let delta = if *first_chunk {
        *first_chunk = false;
        ChatDelta {
            role: Some("assistant".to_string()),
            content: Some(content.to_string()),
            tool_calls: None,
        }
    } else {
        ChatDelta {
            role: None,
            content: Some(content.to_string()),
            tool_calls: None,
        }
    };

    let chunk = ChatCompletionChunk {
        id: id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: now_unix(),
        model: model.to_string(),
        choices: vec![ChatChunkChoice {
            index: 0,
            delta,
            finish_reason: finish_reason.map(str::to_string),
        }],
    };

    let data = serde_json::to_string(&chunk).unwrap_or_default();
    Ok(Event::default().data(data))
}

fn stop_chunk(id: &str, model: &str) -> Result<Event, Infallible> {
    let chunk = ChatCompletionChunk {
        id: id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: now_unix(),
        model: model.to_string(),
        choices: vec![ChatChunkChoice {
            index: 0,
            delta: ChatDelta {
                role: None,
                content: None,
                tool_calls: None,
            },
            finish_reason: Some("stop".to_string()),
        }],
    };

    let data = serde_json::to_string(&chunk).unwrap_or_default();
    Ok(Event::default().data(data))
}

fn done_event() -> Result<Event, Infallible> {
    Ok(Event::default().data("[DONE]"))
}

// =============================================================================
// Conversation parsing helpers
// =============================================================================

/// Extract the last user message content from the message history.
fn extract_last_user_message(messages: &[MessageSlice]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.clone())
        .filter(|s| !s.trim().is_empty())
}

/// Extract `run_id` and `approval_id` from the sentinel embedded in the last
/// assistant message.
///
/// Sentinel format: `<!-- gglib-run-id:UUID approval_id:UUID -->`
fn extract_sentinel(messages: &[MessageSlice]) -> Option<(String, String)> {
    let last_assistant = messages
        .iter()
        .rev()
        .find(|m| m.role == "assistant")?
        .content
        .as_deref()?;

    // Parse sentinel: <!-- gglib-run-id:RUNID approval_id:APPID -->
    let start = last_assistant.find("<!-- gglib-run-id:")?;
    let end = last_assistant[start..].find(" -->")?;
    let inner = &last_assistant[start + 4..start + end]; // strip <!-- and -->

    let mut run_id = None;
    let mut approval_id = None;
    for part in inner.split_whitespace() {
        if let Some(v) = part.strip_prefix("gglib-run-id:") {
            run_id = Some(v.to_string());
        } else if let Some(v) = part.strip_prefix("approval_id:") {
            approval_id = Some(v.to_string());
        }
    }

    Some((run_id?, approval_id?))
}

/// Approval intent parsed from the last user message.
#[derive(Debug)]
enum ApprovalIntent {
    /// User approved, optionally with edit instructions.
    Approve(Option<String>),
    /// User rejected, optionally with a reason.
    Reject(Option<String>),
}

/// Parse the last user message for approval intent.
///
/// - `yes` / `approve` / `ok` / `looks good` → `Approve(None)`
/// - `edit: …` / `revise: …` → `Approve(Some(instructions))`
/// - `no` / `cancel` / `reject` / `stop` → `Reject(None)`
/// - Anything else → defaults to `Approve(None)` (to avoid blocking unknown phrasing)
fn extract_approval_intent(messages: &[MessageSlice]) -> ApprovalIntent {
    let text = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .and_then(|m| m.content.as_deref())
        .unwrap_or("")
        .trim()
        .to_lowercase();

    if let Some(instructions) = text
        .strip_prefix("edit:")
        .or_else(|| text.strip_prefix("revise:"))
        .or_else(|| text.strip_prefix("change:"))
    {
        return ApprovalIntent::Approve(Some(instructions.trim().to_string()));
    }

    let reject_words = ["no", "cancel", "reject", "stop", "abort", "don't", "dont"];
    for word in reject_words {
        if text == word || text.starts_with(&format!("{word} ")) {
            return ApprovalIntent::Reject(None);
        }
    }

    ApprovalIntent::Approve(None)
}

// =============================================================================
// Helpers
// =============================================================================

/// Drop guard that cancels a [`CancellationToken`] when dropped.
///
/// Attached to the SSE stream so that the orchestrator task is aborted
/// when the client disconnects and the stream is dropped.
struct DropCancels(CancellationToken);

impl Drop for DropCancels {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Build a [`ModelInfo`] entry for a virtual model.
pub fn virtual_model_info(name: &str, description: &str) -> ModelInfo {
    ModelInfo {
        id: name.to_string(),
        object: "model".to_string(),
        created: 0,
        owned_by: "gglib".to_string(),
        description: Some(description.to_string()),
    }
}
