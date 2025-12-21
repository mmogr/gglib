//! Download handlers - queue management and HF downloads.

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};

use crate::error::HttpError;
use crate::state::AppState;
use gglib_core::download::QueueSnapshot;

/// Request to queue a download.
#[derive(Debug, Deserialize)]
pub struct QueueDownloadRequest {
    pub model_id: String,
    /// Quantization to download. Accepts both "quant" and "quantization" field names
    /// for compatibility with different frontends (Tauri uses "quantization", legacy uses "quant").
    #[serde(alias = "quantization")]
    pub quant: Option<String>,
}

/// Response from queue_download.
/// Canonical shape returned to all clients - never a tuple.
#[derive(Debug, Serialize)]
pub struct QueueDownloadResponse {
    /// Position in the queue (0 = downloading now).
    pub position: usize,
    /// Number of shards queued (1 for single file, N for sharded models).
    pub shard_count: usize,
}

/// Request to reorder a single download.
#[derive(Debug, Deserialize)]
pub struct ReorderRequest {
    pub model_id: String,
    pub position: usize,
}

/// Request to reorder the entire queue.
#[derive(Debug, Deserialize)]
pub struct ReorderFullRequest {
    pub ids: Vec<String>,
}

/// Get the current download queue.
pub async fn list(State(state): State<AppState>) -> Json<QueueSnapshot> {
    let snapshot = state.gui.get_download_queue().await;

    tracing::debug!(
        target: "gglib.download",
        active_count = snapshot.active_count,
        pending_count = snapshot.pending_count,
        total_items = snapshot.items.len(),
        items = ?snapshot.items.iter().map(|i| (&i.id, &i.status)).collect::<Vec<_>>(),
        "Queue snapshot returned from /api/downloads/queue",
    );

    Json(snapshot)
}

/// Queue a new download.
pub async fn queue(
    State(state): State<AppState>,
    Json(req): Json<QueueDownloadRequest>,
) -> Result<Json<QueueDownloadResponse>, HttpError> {
    let (position, shard_count) = state.gui.queue_download(req.model_id, req.quant).await?;
    Ok(Json(QueueDownloadResponse {
        position,
        shard_count,
    }))
}

/// Remove a pending download from the queue.
pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), HttpError> {
    state.gui.remove_from_download_queue(&id).await?;
    Ok(())
}

/// Cancel an active download.
///
/// This endpoint is idempotent: returns 204 No Content whether or not
/// the download exists. This prevents client-side errors during race
/// conditions (e.g., SSE removes download while cancel is in-flight).
pub async fn cancel(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), HttpError> {
    match state.gui.cancel_download(&id).await {
        Ok(()) => Ok(()),
        // Treat NotFound as success (idempotent cancel)
        Err(gglib_gui::GuiError::NotFound { .. }) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Reorder a single download in the queue.
pub async fn reorder(
    State(state): State<AppState>,
    Json(req): Json<ReorderRequest>,
) -> Result<Json<usize>, HttpError> {
    Ok(Json(
        state
            .gui
            .reorder_download_queue(&req.model_id, req.position)
            .await?,
    ))
}

/// Reorder the entire download queue.
pub async fn reorder_full(
    State(state): State<AppState>,
    Json(req): Json<ReorderFullRequest>,
) -> Result<(), HttpError> {
    state.gui.reorder_download_queue_full(&req.ids).await?;
    Ok(())
}

/// Cancel all shards in a shard group.
pub async fn cancel_shard_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), HttpError> {
    state.gui.cancel_shard_group(&id).await?;
    Ok(())
}

/// Clear all failed downloads.
pub async fn clear_failed(State(state): State<AppState>) {
    state.gui.clear_failed_downloads().await;
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

    /// Contract test: ensures the response is a JSON object with named fields,
    /// not a tuple. This is the canonical shape expected by all clients.
    #[test]
    fn queue_response_has_named_fields() {
        let response = QueueDownloadResponse {
            position: 2,
            shard_count: 4,
        };
        let json = serde_json::to_value(&response).unwrap();

        // Must be an object, not an array (tuple)
        assert!(json.is_object());
        assert_eq!(json["position"], 2);
        assert_eq!(json["shard_count"], 4);
    }
}
