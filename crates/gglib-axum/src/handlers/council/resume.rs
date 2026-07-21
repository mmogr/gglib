//! `POST /api/council/runs/{id}/resume` — resume a parked run.
//!
//! Loads the serialized graph from the database, marks remaining nodes as
//! `Pending`, wires in the approval registry, and re-calls [`execute`] so
//! the wave loop continues from where it left off.

use std::convert::Infallible;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use futures_util::StreamExt as _;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use gglib_agent::council::{CouncilConfig, execute};
use gglib_core::domain::council::events::{COUNCIL_EVENT_CHANNEL_CAPACITY, CouncilEvent};
use gglib_core::domain::council::run::CouncilRunStatus;
use gglib_core::domain::council::task_graph::NodeStatus;
use gglib_core::ports::{CouncilApprovalRegistryPort, CouncilRepositoryPort};
use gglib_core::request_pipeline;
use gglib_runtime::compose_council_ports;

use crate::error::HttpError;
use crate::handlers::port_utils::validate_port;
use crate::state::AppState;

// ─── DTO ─────────────────────────────────────────────────────────────────────

/// Request body for `POST /api/council/runs/{id}/resume`.
#[derive(Debug, serde::Deserialize)]
pub struct ResumeRequest {
    /// Port of the llama-server to use.
    pub port: u16,
    /// Optional model name override.
    #[serde(default)]
    pub model: Option<String>,
}

// ─── POST /api/council/runs/{id}/resume ─────────────────────────────────

/// Resume an interrupted or awaiting-approval orchestrator run.
///
/// Loads the last known graph from the database, resets Pending nodes to
/// their initial state, and re-runs the executor starting from the remaining
/// work.
///
/// # Errors
///
/// Returns 404 if the run is not found, 400 if the run's graph is missing,
/// or 409 if the run is already completed/failed.
pub async fn resume_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<ResumeRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>, HttpError> {
    let run = state
        .council_repo
        .get_run(&run_id)
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?
        .ok_or_else(|| HttpError::NotFound(format!("run '{run_id}' not found")))?;

    // Only resume runs that haven't finished.
    match run.status {
        CouncilRunStatus::Completed | CouncilRunStatus::Failed => {
            return Err(HttpError::Conflict(format!(
                "run '{run_id}' is already {:?} and cannot be resumed",
                run.status
            )));
        }
        _ => {}
    }

    let graph_json = run.graph_json.ok_or_else(|| {
        HttpError::BadRequest(format!(
            "run '{run_id}' has no saved graph — it may not have reached the planning stage"
        ))
    })?;

    let mut graph = serde_json::from_str(&graph_json).map_err(|e| {
        HttpError::Internal(format!(
            "failed to deserialize graph for run '{run_id}': {e}"
        ))
    })?;

    // Reset any nodes that were not Done back to Pending so the wave loop
    // will pick them up.
    {
        let g: &mut gglib_core::domain::council::task_graph::TaskGraph = &mut graph;
        for node in g.nodes.values_mut() {
            if node.status != NodeStatus::Done {
                node.status = NodeStatus::Pending;
            }
        }
    }

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
    );

    let config = CouncilConfig {
        approval_registry: Some(
            Arc::clone(&state.approval_registry) as Arc<dyn CouncilApprovalRegistryPort>
        ),
        repository: Some(
            Arc::clone(&state.council_repo) as Arc<dyn gglib_core::ports::CouncilRepositoryPort>
        ),
        run_id: Some(run_id.clone()),
        graph_override: Some(graph),
        ..CouncilConfig::default()
    };

    let (tx, rx) = mpsc::channel::<CouncilEvent>(COUNCIL_EVENT_CHANNEL_CAPACITY);
    let goal = run.goal.clone();

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
            tracing::error!(error = %e, run_id, "council: resume failed");
        }
    });

    let stream = ReceiverStream::new(rx).map(|event| {
        let data = serde_json::to_string(&event).unwrap_or_default();
        Ok::<_, Infallible>(Event::default().data(data))
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
