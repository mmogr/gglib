#![doc = include_str!("README.md")]
pub mod approve;
pub mod note;
pub mod resume;
pub mod rewind;
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

use gglib_agent::council::estimator::estimate_run_cost;
use gglib_agent::council::plan;
use gglib_core::domain::council::events::{COUNCIL_EVENT_CHANNEL_CAPACITY, CouncilEvent};
use gglib_core::domain::council::task_graph::HitlMode;
use gglib_core::request_pipeline;
use gglib_runtime::compose_council_ports;

use crate::error::HttpError;
use crate::handlers::port_utils::validate_port;
use crate::state::AppState;

// ─── DTO ─────────────────────────────────────────────────────────────────────

/// Request body for `POST /api/council/plan`.
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

// ─── POST /api/council/plan ─────────────────────────────────────────────

/// Stream a director planning pass as [`CouncilEvent`] SSE frames.
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

    let model_context =
        request_pipeline::resolve(state.catalog.as_ref(), req.model.as_deref()).await;
    let ports = compose_council_ports(
        format!("http://127.0.0.1:{}", req.port),
        state.http_client.clone(),
        req.model.clone(),
        model_context,
        state.mcp.clone(),
        None,
        None,
        None,
    );

    let (tx, rx) = mpsc::channel::<CouncilEvent>(COUNCIL_EVENT_CHANNEL_CAPACITY);
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
                let _ = tx.send(CouncilEvent::PlanProposed { graph }).await;
                let _ = tx
                    .send(CouncilEvent::RunCostEstimate {
                        node_count: cost.node_count,
                        est_tokens: cost.est_tokens,
                        est_wall_seconds: cost.est_wall_seconds,
                    })
                    .await;
                let _ = tx
                    .send(CouncilEvent::CouncilComplete { answer: summary })
                    .await;
            }
            Err(e) => {
                let _ = tx
                    .send(CouncilEvent::CouncilError {
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
                tracing::error!(error = %e, "council: failed to serialise CouncilEvent");
                let json =
                    r#"{"type":"orchestrator_error","message":"serialization failed"}"#.to_owned();
                Some(Ok::<Event, Infallible>(Event::default().data(json)))
            }
        })
    });

    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::default()))
}
