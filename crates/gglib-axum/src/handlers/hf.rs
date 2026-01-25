//! HuggingFace handlers - model search and metadata.

use axum::Json;
use axum::extract::{Path, State};

use crate::error::HttpError;
use crate::state::AppState;
use gglib_gui::types::{
    HfModelSummary, HfQuantizationsResponse, HfSearchRequest, HfSearchResponse,
    HfToolSupportResponse,
};

/// Search HuggingFace for GGUF models.
pub async fn search(
    State(state): State<AppState>,
    Json(req): Json<HfSearchRequest>,
) -> Result<Json<HfSearchResponse>, HttpError> {
    Ok(Json(state.gui.browse_hf_models(req).await?))
}

/// Get available quantizations for a model.
pub async fn quantizations(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Result<Json<HfQuantizationsResponse>, HttpError> {
    Ok(Json(state.gui.get_model_quantizations(&model_id).await?))
}

/// Check if a model supports tool/function calling.
pub async fn tool_support(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Result<Json<HfToolSupportResponse>, HttpError> {
    Ok(Json(state.gui.get_hf_tool_support(&model_id).await?))
}

/// Get model summary by exact repo ID (direct API lookup).
///
/// Uses wildcard path to capture the full `owner/repo` format including slashes.
pub async fn model_summary(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Result<Json<HfModelSummary>, HttpError> {
    Ok(Json(state.gui.get_model_summary(&model_id).await?))
}
