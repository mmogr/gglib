//! `POST /api/council/approve/{approval_id}` — resolve a HITL gate.
//!
//! The executor parks awaiting a response from this endpoint.  The body
//! specifies whether to approve as-is, approve with an edited graph, or
//! reject with a reason.

use axum::Json;
use axum::extract::{Path, State};

use gglib_core::domain::council::task_graph::TaskGraph;
use gglib_core::ports::{ApprovalDecision, CouncilApprovalRegistryPort};

use crate::error::HttpError;
use crate::state::AppState;

// ─── DTO ─────────────────────────────────────────────────────────────────────

/// Request body for `POST /api/council/approve/{approval_id}`.
#[derive(Debug, serde::Deserialize)]
pub struct ApproveRequest {
    /// Decision: `"approve"`, `"approve_with_edits"`, or `"reject"`.
    pub decision: ApproveDecision,
    /// Edited task graph (required when `decision == "approve_with_edits"`).
    #[serde(default)]
    pub edited_graph: Option<TaskGraph>,
    /// Human-readable rejection reason (used when `decision == "reject"`).
    #[serde(default)]
    pub reason: Option<String>,
}

/// Discriminant for the approval decision.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApproveDecision {
    Approve,
    ApproveWithEdits,
    Reject,
}

/// Response body for `POST /api/council/approve/{approval_id}`.
#[derive(Debug, serde::Serialize)]
pub struct ApproveResponse {
    /// Whether the registry found and resolved the pending approval.
    pub resolved: bool,
}

// ─── POST /api/council/approve/{approval_id} ────────────────────────────

/// Resolve a pending HITL gate.
///
/// # Errors
///
/// Returns 400 if `approve_with_edits` is requested without providing an
/// edited graph, or if the approval_id is unknown (already resolved or
/// expired).
pub async fn approve(
    State(state): State<AppState>,
    Path(approval_id): Path<String>,
    Json(req): Json<ApproveRequest>,
) -> Result<Json<ApproveResponse>, HttpError> {
    let decision = match req.decision {
        ApproveDecision::Approve => ApprovalDecision::Approve,
        ApproveDecision::ApproveWithEdits => {
            let graph = req.edited_graph.ok_or_else(|| {
                HttpError::BadRequest(
                    "edited_graph is required when decision is 'approve_with_edits'".into(),
                )
            })?;
            ApprovalDecision::ApproveWithEdits(Box::new(graph))
        }
        ApproveDecision::Reject => ApprovalDecision::Reject(req.reason.unwrap_or_default()),
    };

    let resolved = state.approval_registry.resolve(&approval_id, decision);

    if !resolved {
        return Err(HttpError::NotFound(format!(
            "approval '{approval_id}' not found — it may have already been resolved"
        )));
    }

    Ok(Json(ApproveResponse { resolved: true }))
}
