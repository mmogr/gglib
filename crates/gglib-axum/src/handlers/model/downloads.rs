//! Download handlers - queue management and HF downloads.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;

use crate::error::HttpError;
use crate::state::AppState;
use gglib_app_services::types::QueueDownloadResponse;
use gglib_core::download::QueueSnapshot;

/// Request to queue a download.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct QueueDownloadRequest {
    pub model_id: String,
    /// Quantization to download. Accepts both "quant" and "quantization" field names
    /// for compatibility with different frontends (Tauri uses "quantization", legacy uses "quant").
    #[serde(alias = "quantization")]
    pub quant: Option<String>,
}

/// Request to reorder a single download.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct ReorderRequest {
    pub model_id: String,
    pub position: usize,
}

/// Request to reorder the entire queue.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct ReorderFullRequest {
    pub ids: Vec<String>,
}

/// Get the current download queue.
pub(crate) async fn list(State(state): State<AppState>) -> Json<QueueSnapshot> {
    let snapshot = state.downloads.get_queue_snapshot().await;

    tracing::debug!(
        target: "gglib.download",
        revision = snapshot.revision,
        active = ?snapshot.active.as_ref().map(|row| (&row.id, row.phase)),
        waiting = snapshot.waiting.len(),
        finished = snapshot.finished.len(),
        "Queue snapshot returned from /api/downloads/queue",
    );

    Json(snapshot)
}

/// Queue a new download, and answer its ID.
pub(crate) async fn queue(
    State(state): State<AppState>,
    Json(req): Json<QueueDownloadRequest>,
) -> Result<Json<QueueDownloadResponse>, HttpError> {
    let queued = state
        .downloads
        .queue_download(req.model_id, req.quant)
        .await?;
    Ok(Json(queued))
}

/// Take a download off the queue: cancel it when it is waiting or running,
/// and otherwise drop its finished entry.
pub(crate) async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), HttpError> {
    state.downloads.remove_from_queue(&id).await?;
    Ok(())
}

/// Cancel a download that is waiting or running, every file of it.
///
/// This endpoint is idempotent: it answers 200 whether or not the download
/// is in the queue. This prevents client-side errors during race
/// conditions (e.g., SSE removes download while cancel is in-flight).
#[allow(
    clippy::match_same_arms,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) async fn cancel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), HttpError> {
    match state.downloads.cancel_download(&id).await {
        Ok(()) => Ok(()),
        // Treat NotFound as success (idempotent cancel)
        Err(gglib_app_services::GuiError::NotFound { .. }) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Reorder a single download in the queue.
pub(crate) async fn reorder(
    State(state): State<AppState>,
    Json(req): Json<ReorderRequest>,
) -> Result<Json<usize>, HttpError> {
    Ok(Json(
        state
            .downloads
            .reorder_queue(&req.model_id, req.position)
            .await?,
    ))
}

/// Reorder the entire download queue.
pub(crate) async fn reorder_full(
    State(state): State<AppState>,
    Json(req): Json<ReorderFullRequest>,
) -> Result<(), HttpError> {
    state.downloads.reorder_queue_full(&req.ids).await?;
    Ok(())
}

/// Clear the record of how earlier downloads ended.
pub(crate) async fn clear_finished(State(state): State<AppState>) {
    state.downloads.clear_finished().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Contract test: ensures the HTTP API accepts both "quant" and "quantization" field names.
    /// This prevents regression of the field name mismatch bug where the frontend sends
    /// "quantization" but the backend expected "quant".
    #[test]
    fn queue_request_accepts_quantization_field() {
        let json = serde_json::json!({
            "model_id": "test/model",
            "quantization": "Q8_0"
        });

        let req: QueueDownloadRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.model_id, "test/model");
        assert_eq!(req.quant.as_deref(), Some("Q8_0"));
    }

    #[test]
    fn queue_request_accepts_quant_field() {
        let json = serde_json::json!({
            "model_id": "test/model",
            "quant": "Q4_K_M"
        });

        let req: QueueDownloadRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.model_id, "test/model");
        assert_eq!(req.quant.as_deref(), Some("Q4_K_M"));
    }

    #[test]
    fn queue_request_allows_missing_quant() {
        let json = serde_json::json!({
            "model_id": "test/model"
        });

        let req: QueueDownloadRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.model_id, "test/model");
        assert!(req.quant.is_none());
    }

    /// When both fields are present, serde rejects the request as a duplicate field error.
    /// This is the correct behavior - clients should use only one field name.
    #[test]
    fn queue_request_rejects_both_quant_and_quantization() {
        let json = serde_json::json!({
            "model_id": "test/model",
            "quant": "Q4_K_M",
            "quantization": "Q8_0"
        });

        let result: Result<QueueDownloadRequest, _> = serde_json::from_value(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("duplicate field"));
    }
}
