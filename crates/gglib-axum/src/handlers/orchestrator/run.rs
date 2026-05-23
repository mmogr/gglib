//! `POST /api/orchestrator/run` — execute a full Director/Worker task graph.
//!
//! Accepts a [`RunRequest`] JSON body, runs the complete orchestrator pipeline
//! (planning → worker execution → compaction → synthesis), and streams
//! [`OrchestratorEvent`]s as newline-delimited JSON SSE frames.
//!
//! # Event sequence
//!
//! 1. Zero or more [`OrchestratorEvent::ReplanAttempt`] events.
//! 2. [`OrchestratorEvent::PlanProposed`] containing the validated graph.
//! 3. [`OrchestratorEvent::PlanApproved`] (auto-approved in Phase C).
//! 4. Per-node: `NodeStarted → NodeTextDelta* → NodeToolCall* → NodeCompacting → NodeComplete`.
//! 5. `SynthesisStart → SynthesisTextDelta* → SynthesisComplete`.
//! 6. [`OrchestratorEvent::OrchestratorComplete`] with the final answer.
//!
//! On failure: [`OrchestratorEvent::NodeFailed`] for the failed node, then
//! [`OrchestratorEvent::OrchestratorError`] and the stream closes.

use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use futures_util::StreamExt as _;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use gglib_agent::orchestrator::{OrchestratorConfig, execute};
use gglib_core::domain::orchestrator::events::{
    ORCHESTRATOR_EVENT_CHANNEL_CAPACITY, OrchestratorEvent,
};
use gglib_core::domain::orchestrator::run::{OrchestratorRun, OrchestratorRunStatus};
use gglib_core::domain::orchestrator::task_graph::HitlMode;
use gglib_core::ports::{OrchestratorApprovalRegistryPort, OrchestratorRepositoryPort};
use gglib_runtime::compose_council_ports;
use std::sync::Arc;

use crate::error::HttpError;
use crate::handlers::port_utils::validate_port;
use crate::state::AppState;

// ─── DTO ─────────────────────────────────────────────────────────────────────

/// Request body for `POST /api/orchestrator/run`.
#[derive(Debug, serde::Deserialize)]
pub struct RunRequest {
    /// High-level goal to decompose and execute.
    pub goal: String,
    /// Port of the llama-server to use.
    pub port: u16,
    /// Optional model name override.
    #[serde(default)]
    pub model: Option<String>,
    /// Maximum number of director replan attempts after the first.
    ///
    /// Defaults to `2` when omitted.
    #[serde(default = "default_max_replans")]
    pub max_replans: u32,
    /// Maximum number of worker nodes to run concurrently.
    ///
    /// Defaults to `3` when omitted.
    #[serde(default = "default_max_worker_concurrency")]
    pub max_worker_concurrency: usize,
    /// Human-in-the-loop mode.
    ///
    /// Defaults to [`HitlMode::None`] (auto-approve) when omitted.
    #[serde(default)]
    pub hitl_mode: HitlMode,
}

fn default_max_replans() -> u32 {
    2
}

fn default_max_worker_concurrency() -> usize {
    3
}

// ─── POST /api/orchestrator/run ──────────────────────────────────────────────

/// Stream a full orchestrator run as [`OrchestratorEvent`] SSE frames.
///
/// # Errors
///
/// Returns an HTTP error when the port is invalid, the server on that port
/// is unreachable, or the agent semaphore is already at capacity.
pub async fn run_sse(
    State(state): State<AppState>,
    Json(req): Json<RunRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>, HttpError> {
    let permit = state
        .agent_semaphore
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            HttpError::TooManyRequests("all agent loop slots are in use; try again later".into())
        })?;

    validate_port(&state, req.port).await?;

    let tags = match req.model.as_deref() {
        Some(name) => state.core.models().tags_for(name).await,
        None => Vec::new(),
    };
    let ports = compose_council_ports(
        format!("http://127.0.0.1:{}", req.port),
        state.http_client.clone(),
        req.model.clone(),
        tags,
        state.mcp.clone(),
        None,
    );

    let config = OrchestratorConfig {
        max_replans: req.max_replans,
        max_worker_concurrency: req.max_worker_concurrency,
        hitl_mode: req.hitl_mode.clone(),
        approval_registry: Some(
            Arc::clone(&state.approval_registry) as Arc<dyn OrchestratorApprovalRegistryPort>
        ),
        repository: Some(
            Arc::clone(&state.orchestrator_repo) as Arc<dyn OrchestratorRepositoryPort>
        ),
        ..OrchestratorConfig::default()
    };

    // Pre-generate run_id and register a steering note queue for this run.
    let run_id = uuid::Uuid::new_v4().to_string();
    let note_queue: Arc<tokio::sync::Mutex<Vec<String>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));
    {
        state
            .steering_note_queues
            .lock()
            .await
            .insert(run_id.clone(), Arc::clone(&note_queue));
    }

    let config = OrchestratorConfig {
        run_id: Some(run_id.clone()),
        note_queue: Some(Arc::clone(&note_queue)),
        ..config
    };

    // Create the run record now so that append_event calls inside execute()
    // can satisfy the orchestrator_events FK constraint.  The executor skips
    // create_run when run_id is pre-set (it assumes the caller already did
    // this), which is the correct resume-path behaviour — but for a fresh run
    // we must do it here.
    {
        let now = chrono::Utc::now().to_rfc3339();
        let run = OrchestratorRun {
            id: run_id.clone(),
            goal: req.goal.clone(),
            graph_json: None,
            status: OrchestratorRunStatus::Running,
            hitl_mode: req.hitl_mode.clone(),
            conversation_id: None,
            created_at: now.clone(),
            updated_at: now,
        };
        if let Err(e) = state.orchestrator_repo.create_run(run).await {
            tracing::warn!(run_id, error = %e, "orchestrator: failed to pre-create run record");
        }
    }

    let (tx, rx) = mpsc::channel::<OrchestratorEvent>(ORCHESTRATOR_EVENT_CHANNEL_CAPACITY);
    let goal = req.goal.clone();
    let steering_queues = Arc::clone(&state.steering_note_queues);

    tokio::spawn(async move {
        let _permit = permit;

        if let Err(e) = execute(
            &goal,
            &[],
            ports.llm,
            ports.tool_executor,
            config,
            tx.clone(),
        )
        .await
        {
            // execute() already sent OrchestratorError on the channel before
            // returning Err, so we only log here for server-side observability.
            tracing::error!(error = %e, goal, "orchestrator: run failed");
        }

        // Clean up the steering note queue for this run.
        steering_queues.lock().await.remove(&run_id);
    });

    let sse_stream = ReceiverStream::new(rx).filter_map(|event| {
        futures_util::future::ready(match serde_json::to_string(&event) {
            Ok(json) => Some(Ok::<Event, Infallible>(Event::default().data(json))),
            Err(e) => {
                tracing::error!(error = %e, "orchestrator: failed to serialise OrchestratorEvent");
                let json =
                    r#"{"type":"orchestrator_error","message":"serialization failed"}"#.to_owned();
                Some(Ok::<Event, Infallible>(Event::default().data(json)))
            }
        })
    });

    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}
