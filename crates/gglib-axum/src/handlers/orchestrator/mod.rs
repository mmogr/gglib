//! Orchestrator endpoints: decompose a goal into a task-graph plan via SSE, and
//! execute the full Director/Worker pipeline.
//!
//! # Route
//!
//! `POST /api/orchestrator/plan` — accepts a [`PlanRequest`] JSON body and
//! streams [`OrchestratorEvent`]s as newline-delimited JSON SSE frames.
//!
//! # Event sequence
//!
//! 1. Zero or more [`OrchestratorEvent::ReplanAttempt`] events if the
//!    director retries validation.
//! 2. [`OrchestratorEvent::PlanProposed`] containing the validated
//!    [`TaskGraph`].
//! 3. [`OrchestratorEvent::OrchestratorComplete`] with a brief summary.
//!
//! On failure the stream emits [`OrchestratorEvent::OrchestratorError`] then
//! closes.

pub mod approve;
pub mod note;
pub mod resume;
pub mod run;
pub mod runs;
pub mod steer;

use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use futures_util::StreamExt as _;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use gglib_agent::orchestrator::estimator::estimate_run_cost;
use gglib_agent::orchestrator::plan;
use gglib_core::domain::orchestrator::events::{
    ORCHESTRATOR_EVENT_CHANNEL_CAPACITY, OrchestratorEvent,
};
use gglib_core::domain::orchestrator::task_graph::HitlMode;
use gglib_runtime::compose_council_ports;

use crate::error::HttpError;
use crate::handlers::port_utils::validate_port;
use crate::state::AppState;

// ─── DTO ─────────────────────────────────────────────────────────────────────

/// Request body for `POST /api/orchestrator/plan`.
#[derive(Debug, serde::Deserialize)]
pub struct PlanRequest {
    /// High-level goal to decompose into a task graph.
    pub goal: String,
    /// Port of the llama-server to use for the director LLM call.
    pub port: u16,
    /// Optional model name override.
    #[serde(default)]
    pub model: Option<String>,
    /// Maximum number of replan attempts after the first.
    ///
    /// Defaults to `2` when omitted.
    #[serde(default = "default_max_replans")]
    pub max_replans: u32,
}

fn default_max_replans() -> u32 {
    2
}

// ─── POST /api/orchestrator/plan ─────────────────────────────────────────────

/// Stream a director planning pass as [`OrchestratorEvent`] SSE frames.
///
/// # Errors
///
/// Returns an HTTP error when the port is invalid, there is no running
/// server on that port, or the agent semaphore is full.
pub async fn plan_sse(
    State(state): State<AppState>,
    Json(req): Json<PlanRequest>,
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

    let (tx, rx) = mpsc::channel::<OrchestratorEvent>(ORCHESTRATOR_EVENT_CHANNEL_CAPACITY);
    let goal = req.goal.clone();
    let max_replans = req.max_replans;

    tokio::spawn(async move {
        let _permit = permit;

        match plan(
            &goal,
            &[],
            ports.llm,
            HitlMode::None,
            max_replans,
            Some(tx.clone()),
        )
        .await
        {
            Ok(graph) => {
                let summary = format!("Plan accepted: {} node(s)", graph.nodes.len());
                let cost = estimate_run_cost(&graph);
                let _ = tx.send(OrchestratorEvent::PlanProposed { graph }).await;
                let _ = tx
                    .send(OrchestratorEvent::RunCostEstimate {
                        node_count: cost.node_count,
                        est_tokens: cost.est_tokens,
                        est_wall_seconds: cost.est_wall_seconds,
                    })
                    .await;
                let _ = tx
                    .send(OrchestratorEvent::OrchestratorComplete { answer: summary })
                    .await;
            }
            Err(e) => {
                let _ = tx
                    .send(OrchestratorEvent::OrchestratorError {
                        message: e.to_string(),
                    })
                    .await;
            }
        }
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
