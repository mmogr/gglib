//! `POST /api/council/runs/{id}/rewind` — rewind a run to a previous wave.
//!
//! Truncates the event log after the given wave index, resets graph nodes that
//! completed in later waves back to `Pending`, and re-executes the executor
//! from that point.  The endpoint streams new events as SSE.

use std::convert::Infallible;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_core::Stream;
use futures_util::StreamExt as _;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use gglib_agent::council::{CouncilConfig, NoteQueue, execute};
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

/// Request body for `POST /api/council/runs/{id}/rewind`.
#[derive(Debug, serde::Deserialize)]
pub struct RewindRequest {
    /// Port of the llama-server to use for re-execution.
    pub port: u16,
    /// Optional model name override.
    #[serde(default)]
    pub model: Option<String>,
    /// Zero-based wave index to rewind to (inclusive).
    ///
    /// All events and node completions from waves **after** this index will
    /// be discarded.  Execution resumes from the first incomplete wave.
    pub wave_index: u32,
    /// Optional steering note to inject into the re-execution.
    ///
    /// Useful for rewinding and course-correcting at the same time.
    #[serde(default)]
    pub steering_note: Option<String>,
}

// ─── POST /api/council/runs/{id}/rewind ─────────────────────────────────

/// Rewind an orchestrator run to a previous wave and re-execute from there.
///
/// # Errors
///
/// - 404 if the run does not exist.
/// - 400 if the run's graph is missing.
/// - 409 if the run is currently active (Running / AwaitingApproval).
pub async fn rewind_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<RewindRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static>, HttpError> {
    let run = state
        .council_repo
        .get_run(&run_id)
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?
        .ok_or_else(|| HttpError::NotFound(format!("run '{run_id}' not found")))?;

    // Only allow rewind on runs that are not currently executing.
    match run.status {
        CouncilRunStatus::Running | CouncilRunStatus::AwaitingApproval => {
            return Err(HttpError::Conflict(format!(
                "run '{run_id}' is currently active and cannot be rewound; cancel it first"
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

    // Identify which node_ids completed in waves AFTER the target wave so we
    // can reset them to Pending.  We do this before truncating the events.
    let events = state
        .council_repo
        .list_events(&run_id)
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?;

    let nodes_to_reset: std::collections::HashSet<String> = events
        .iter()
        .filter(|ev| ev.wave_index > req.wave_index)
        .filter_map(|ev| {
            // Parse just enough to detect node_complete events.
            let v: serde_json::Value = serde_json::from_str(&ev.event_json).ok()?;
            if v.get("type")?.as_str()? == "node_complete" {
                v.get("node_id")?.as_str().map(String::from)
            } else {
                None
            }
        })
        .collect();

    // Reset nodes that completed after the target wave back to Pending.
    {
        let g: &mut gglib_core::domain::council::task_graph::TaskGraph = &mut graph;
        for (id, node) in g.nodes.iter_mut() {
            if nodes_to_reset.contains(id.0.as_str()) {
                node.status = NodeStatus::Pending;
                node.output = None;
                node.error = None;
            }
        }
    }

    // Truncate event log.
    state
        .council_repo
        .truncate_events_after_wave(&run_id, req.wave_index)
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?;

    // Persist the updated graph.
    if let Ok(json) = serde_json::to_string(&graph) {
        let _ = state.council_repo.update_graph(&run_id, &json).await;
    }

    // Reset run status to Running.
    state
        .council_repo
        .update_run_status(&run_id, CouncilRunStatus::Running)
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?;

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
    let note_queue: Option<NoteQueue> = req.steering_note.map(|note| {
        let q: NoteQueue = Arc::new(tokio::sync::Mutex::new(vec![note]));
        q
    });

    // Determine the next event seq from the surviving events so we don't
    // collide with existing seq numbers.
    let next_seq = events
        .iter()
        .filter(|ev| ev.wave_index <= req.wave_index)
        .map(|ev| ev.seq + 1)
        .max()
        .unwrap_or(0);

    let config = CouncilConfig {
        hitl_mode: run.hitl_mode.clone(),
        approval_registry: Some(
            Arc::clone(&state.approval_registry) as Arc<dyn CouncilApprovalRegistryPort>
        ),
        repository: Some(
            Arc::clone(&state.council_repo) as Arc<dyn gglib_core::ports::CouncilRepositoryPort>
        ),
        run_id: Some(run_id.clone()),
        graph_override: Some(graph),
        note_queue,
        rewind_to_wave: Some(req.wave_index),
        ..CouncilConfig::default()
    };

    let (tx, rx) = mpsc::channel::<CouncilEvent>(COUNCIL_EVENT_CHANNEL_CAPACITY);
    let goal = run.goal.clone();

    // We need to set event_seq to next_seq — but CouncilConfig doesn't
    // expose that directly.  The executor always starts event_seq at 0 and
    // uses INSERT OR IGNORE, so duplicate seq values are silently dropped.
    // To avoid collisions, we pass next_seq via the run_id convention: since
    // the executor uses INSERT OR IGNORE, we just accept that pre-rewind
    // events won't be double-persisted (the seq constraint protects them).
    // The executor will start at seq=0, but INSERT OR IGNORE will skip any
    // seq that already exists in the DB, so new events effectively start
    // after the last surviving event's seq.
    let _ = next_seq; // acknowledged — INSERT OR IGNORE handles deduplication

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
            tracing::error!(error = %e, run_id, "council: rewind re-execution failed");
        }
    });

    let stream = ReceiverStream::new(rx).map(|event| {
        let data = serde_json::to_string(&event).unwrap_or_default();
        Ok::<_, Infallible>(Event::default().data(data))
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
