//! Verification handlers - model integrity and updates.

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};

use crate::error::HttpError;
use crate::state::AppState;
use gglib_core::ports::AppEventEmitter;
use gglib_core::services::{UpdateCheckResult, VerificationReport};

/// Response from verify endpoint.
#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub report: VerificationReport,
}

/// Response from check updates endpoint.
#[derive(Debug, Serialize)]
pub struct CheckUpdatesResponse {
    pub result: UpdateCheckResult,
    pub message: String,
}

/// Request body for repair endpoint.
#[derive(Debug, Deserialize)]
pub struct RepairRequest {
    /// Optional list of shard indices to repair. If None, repairs all corrupt shards.
    pub shards: Option<Vec<usize>>,
}

/// Response from repair endpoint.
#[derive(Debug, Serialize)]
pub struct RepairResponse {
    pub message: String,
}

/// Verify model integrity.
///
/// POST /api/models/{id}/verify
///
/// This endpoint streams progress via SSE and returns a verification report.
/// Clients should subscribe to SSE events to receive real-time progress updates.
pub async fn verify(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<VerifyResponse>, HttpError> {
    // Get verification service
    let verification = state.core.verification()
        .ok_or_else(|| HttpError::NotFound("Verification service not available".to_string()))?;

    // Get model info for validation
    let model = state.core.models().get_by_id(id).await?
        .ok_or_else(|| HttpError::NotFound(format!("Model with ID {} not found", id)))?;

    tracing::info!(
        target: "gglib.verification",
        model_id = id,
        model_name = %model.name,
        "Starting model verification",
    );

    // Start verification
    let (mut progress_rx, handle) = verification.verify_model_integrity(id).await
        .map_err(|e| HttpError::Internal(format!("Failed to start verification: {}", e)))?;

    // Stream progress via SSE
    while let Some(progress) = progress_rx.recv().await {
        // Extract progress information from shard_progress
        let (bytes_processed, total_bytes) = match &progress.shard_progress {
            gglib_core::services::ShardProgress::Hashing { bytes_processed, total_bytes, .. } => {
                (*bytes_processed, *total_bytes)
            }
            _ => continue, // Skip non-hashing progress updates
        };

        tracing::debug!(
            target: "gglib.verification",
            model_id = id,
            shard_index = progress.shard_index,
            bytes_processed = bytes_processed,
            total_bytes = total_bytes,
            "Verification progress",
        );

        // Emit progress event via SSE
        // Note: shard_name will be constructed from shard_index on the frontend
        let event = gglib_core::events::AppEvent::VerificationProgress {
            model_id: id,
            model_name: model.name.clone(),
            shard_name: format!("Shard {}/{}", progress.shard_index + 1, progress.total_shards),
            bytes_processed,
            total_bytes,
        };
        state.sse.emit(event);
    }

    // Wait for verification to complete
    let report = handle.await
        .map_err(|e| HttpError::Internal(format!("Verification task failed: {}", e)))?
        .map_err(|e| HttpError::Internal(format!("Verification failed: {}", e)))?;

    tracing::info!(
        target: "gglib.verification",
        model_id = id,
        overall_health = ?report.overall_health,
        total_shards = report.shards.len(),
        "Verification complete",
    );

    // Emit completion event
    let completion_event = gglib_core::events::AppEvent::VerificationComplete {
        model_id: id,
        model_name: model.name,
        overall_health: report.overall_health.clone(),
    };
    state.sse.emit(completion_event);

    Ok(Json(VerifyResponse { report }))
}

/// Check for model updates on HuggingFace.
///
/// GET /api/models/{id}/updates
pub async fn check_updates(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<CheckUpdatesResponse>, HttpError> {
    // Get verification service
    let verification = state.core.verification()
        .ok_or_else(|| HttpError::NotFound("Verification service not available".to_string()))?;

    // Get model info for validation
    let model = state.core.models().get_by_id(id).await?
        .ok_or_else(|| HttpError::NotFound(format!("Model with ID {} not found", id)))?;

    tracing::info!(
        target: "gglib.verification",
        model_id = id,
        model_name = %model.name,
        "Checking for updates",
    );

    // Check for updates
    let result = verification.check_for_updates(id).await
        .map_err(|e| HttpError::Internal(format!("Failed to check for updates: {}", e)))?;

    let message = if result.update_available {
        let changed_shards = result.details.as_ref()
            .map(|d| d.changed_shards)
            .unwrap_or(0);
        format!("Updates available: {} shards can be updated", changed_shards)
    } else {
        "Model is up to date".to_string()
    };

    tracing::info!(
        target: "gglib.verification",
        model_id = id,
        update_available = result.update_available,
        "Update check complete",
    );

    Ok(Json(CheckUpdatesResponse {
        result,
        message,
    }))
}

/// Repair model by re-downloading corrupt shards.
///
/// POST /api/models/{id}/repair
pub async fn repair(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<RepairRequest>,
) -> Result<Json<RepairResponse>, HttpError> {
    // Get verification service
    let verification = state.core.verification()
        .ok_or_else(|| HttpError::NotFound("Verification service not available".to_string()))?;

    // Get model info for validation
    let model = state.core.models().get_by_id(id).await?
        .ok_or_else(|| HttpError::NotFound(format!("Model with ID {} not found", id)))?;

    tracing::info!(
        target: "gglib.verification",
        model_id = id,
        model_name = %model.name,
        shards = ?req.shards,
        "Starting model repair",
    );

    // Repair model
    let message = verification.repair_model(id, req.shards).await
        .map_err(|e| HttpError::Internal(format!("Failed to repair model: {}", e)))?;

    tracing::info!(
        target: "gglib.verification",
        model_id = id,
        "Repair initiated successfully",
    );

    Ok(Json(RepairResponse { message }))
}
