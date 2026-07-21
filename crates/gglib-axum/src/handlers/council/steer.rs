//! `POST /api/council/steer` — apply a natural-language steering
//! instruction to a [`TaskGraph`] preview and return the resulting
//! [`GraphDiff`].
//!
//! This is a **stateless, preview-only** endpoint: it does not mutate any
//! running orchestrator.  Callers may inspect the diff and, if satisfied,
//! submit it to a live run via `POST /api/council/runs/{run_id}/note`.

use axum::Json;
use axum::extract::State;
use std::sync::Arc;

use gglib_agent::council::steering::steering_call;
use gglib_core::domain::council::task_graph::{GraphDiff, TaskGraph};
use gglib_core::request_pipeline;
use gglib_runtime::compose_council_ports;

use crate::error::HttpError;
use crate::handlers::port_utils::validate_port;
use crate::state::AppState;

// ─── DTO ─────────────────────────────────────────────────────────────────────

/// Request body for `POST /api/council/steer`.
#[derive(Debug, serde::Deserialize)]
pub struct SteerRequest {
    /// The current task graph to steer.
    pub graph: TaskGraph,
    /// Natural-language instruction describing the desired modification.
    pub instruction: String,
    /// Port of the llama-server to use for the steering LLM call.
    pub port: u16,
    /// Optional model name override.
    #[serde(default)]
    pub model: Option<String>,
}

/// Response body for `POST /api/council/steer`.
#[derive(Debug, serde::Serialize)]
pub struct SteerResponse {
    /// The diff produced by the steering LLM call.
    pub diff: GraphDiff,
}

// ─── POST /api/council/steer ────────────────────────────────────────────

/// Preview a steering instruction against a graph and return the diff.
///
/// # Errors
///
/// Returns an HTTP error when the port is invalid or the LLM call fails.
pub async fn steer(
    State(state): State<AppState>,
    Json(req): Json<SteerRequest>,
) -> Result<Json<SteerResponse>, HttpError> {
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

    let diff = steering_call(&req.graph, &req.instruction, &Arc::clone(&ports.llm))
        .await
        .map_err(|e| HttpError::Internal(e.to_string()))?;

    Ok(Json(SteerResponse { diff }))
}
