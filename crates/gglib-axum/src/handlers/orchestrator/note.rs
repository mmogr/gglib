//! `POST /api/orchestrator/runs/{run_id}/note` — enqueue a natural-language
//! steering instruction for a live orchestrator run.
//!
//! The instruction is appended to the run's [`NoteQueue`].  At the next wave
//! boundary the executor drains the queue, calls the steering LLM, applies
//! the resulting [`GraphDiff`], and emits a
//! [`OrchestratorEvent::SteeringApplied`] event on the SSE stream.
//!
//! Returns `202 Accepted` on success, or `404 Not Found` when no run with
//! the given `run_id` is currently active.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::state::AppState;

// ─── DTO ─────────────────────────────────────────────────────────────────────

/// Request body for `POST /api/orchestrator/runs/{run_id}/note`.
#[derive(Debug, serde::Deserialize)]
pub struct NoteRequest {
    /// Natural-language steering instruction to enqueue.
    pub instruction: String,
}

// ─── POST /api/orchestrator/runs/{run_id}/note ───────────────────────────────

/// Enqueue a steering note for the specified live run.
///
/// Returns `202 Accepted` when the note was enqueued, or `404 Not Found`
/// when the run_id does not correspond to an active run.
pub async fn post_note(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<NoteRequest>,
) -> impl IntoResponse {
    let queues = state.steering_note_queues.lock().await;
    match queues.get(&run_id) {
        Some(queue) => {
            queue.lock().await.push(req.instruction);
            StatusCode::ACCEPTED
        }
        None => StatusCode::NOT_FOUND,
    }
}
