//! Model handlers - CRUD operations for local models.

use axum::Json;
use axum::extract::{Path, Query, State};

use crate::error::HttpError;
use crate::state::AppState;
use gglib_app_services::types::{
    AddModelRequest, GuiModel, ModelDetailDto, RemoveModelRequest, SetCapabilitiesRequest,
    UpdateModelRequest,
};
use gglib_core::domain::{ModelListQuery, ModelSortBy, SortOrder};
use gglib_core::ModelFilterOptions;

// ─────────────────────────────────────────────────────────────────────────────
// Query-parameter struct for GET /api/models
// ─────────────────────────────────────────────────────────────────────────────

/// Flat, HTTP-friendly representation of [`ModelListQuery`].
///
/// Axum's `Query` extractor uses `serde_urlencoded`, which handles primitive
/// fields cleanly.  Multi-value lists (tags, quantizations) are expressed as
/// comma-separated strings so no extra crate is required.
///
/// Examples:
/// ```text
/// GET /api/models
/// GET /api/models?sort=latest_tg_tps&order=desc&min_speed=30
/// GET /api/models?sort=param_count&order=asc&tags=chat,code
/// GET /api/models?quantizations=Q4_K_M,Q8_0&min_params=7&max_params=70
/// ```
#[derive(Debug, Default, serde::Deserialize)]
pub struct ModelListQueryParams {
    /// Sort field. One of `added_at` | `name` | `param_count` | `latest_tg_tps`.
    pub sort: Option<ModelSortBy>,
    /// Sort direction. One of `asc` | `desc`.
    pub order: Option<SortOrder>,
    pub min_params: Option<f64>,
    pub max_params: Option<f64>,
    pub min_context: Option<f64>,
    pub max_context: Option<f64>,
    /// Comma-separated quantization allowlist (e.g. `Q4_K_M,Q8_0`).
    pub quantizations: Option<String>,
    /// Comma-separated required tags (AND semantics, e.g. `chat,code`).
    pub tags: Option<String>,
    pub min_speed: Option<f64>,
    pub max_speed: Option<f64>,
}

impl From<ModelListQueryParams> for ModelListQuery {
    fn from(p: ModelListQueryParams) -> Self {
        Self {
            sort_by: p.sort.unwrap_or_default(),
            order: p.order.unwrap_or_default(),
            min_params: p.min_params,
            max_params: p.max_params,
            min_context: p.min_context,
            max_context: p.max_context,
            quantizations: p
                .quantizations
                .map(|s| s.split(',').map(str::to_string).collect()),
            tags: p.tags.map(|s| s.split(',').map(str::to_string).collect()),
            min_speed: p.min_speed,
            max_speed: p.max_speed,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────────────────────

/// List models, optionally filtered and sorted via query parameters.
pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<ModelListQueryParams>,
) -> Result<Json<Vec<GuiModel>>, HttpError> {
    Ok(Json(state.models.list_with_query(params.into()).await?))
}

/// Get a single model by ID.
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<GuiModel>, HttpError> {
    Ok(Json(state.models.get(id).await?))
}

/// Add a new model from a local file.
pub async fn add(
    State(state): State<AppState>,
    Json(req): Json<AddModelRequest>,
) -> Result<Json<GuiModel>, HttpError> {
    Ok(Json(state.models.add(req).await?))
}

/// Update an existing model.
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateModelRequest>,
) -> Result<Json<GuiModel>, HttpError> {
    Ok(Json(state.models.update(id, req).await?))
}

/// Remove a model.
pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<RemoveModelRequest>,
) -> Result<Json<String>, HttpError> {
    Ok(Json(state.models.remove(id, req).await?))
}

/// Get all unique tags.
pub async fn list_tags(State(state): State<AppState>) -> Result<Json<Vec<String>>, HttpError> {
    Ok(Json(state.models.list_tags().await?))
}

/// Get tags for a specific model.
pub async fn get_model_tags(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<String>>, HttpError> {
    Ok(Json(state.models.get_tags(id).await?))
}

/// Add a tag to a model.
pub async fn add_tag(
    State(state): State<AppState>,
    Path((id, tag)): Path<(i64, String)>,
) -> Result<(), HttpError> {
    state.models.add_tag(id, tag).await?;
    Ok(())
}

/// Request body for adding a tag via POST to /api/models/{id}/tags
#[derive(serde::Deserialize)]
pub struct AddTagRequest {
    pub tag: String,
}

/// Add a tag to a model (body-based version for frontend transport).
pub async fn add_tag_body(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(request): Json<AddTagRequest>,
) -> Result<(), HttpError> {
    state.models.add_tag(id, request.tag).await?;
    Ok(())
}

/// Remove a tag from a model.
pub async fn remove_tag(
    State(state): State<AppState>,
    Path((id, tag)): Path<(i64, String)>,
) -> Result<(), HttpError> {
    state.models.remove_tag(id, tag).await?;
    Ok(())
}

/// Get models with a specific tag.
pub async fn get_by_tag(
    State(state): State<AppState>,
    Path(tag): Path<String>,
) -> Result<Json<Vec<i64>>, HttpError> {
    Ok(Json(state.models.get_by_tag(tag).await?))
}

/// Get filter options for the model list UI.
pub async fn filter_options(
    State(state): State<AppState>,
) -> Result<Json<ModelFilterOptions>, HttpError> {
    Ok(Json(state.models.get_filter_options().await?))
}

pub async fn set_capabilities(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<SetCapabilitiesRequest>,
) -> Result<Json<GuiModel>, HttpError> {
    Ok(Json(state.models.set_capabilities(id, req).await?))
}

/// Get full details for a model by ID (inspect view).
///
/// Returns a [`ModelDetailDto`] containing every stored field — a superset of
/// the [`GuiModel`] returned by `GET /api/models/{id}`.  Includes raw GGUF
/// metadata, MoE topology, full HuggingFace provenance, capability flags,
/// inference defaults, and timestamps.
pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ModelDetailDto>, HttpError> {
    Ok(Json(state.models.get_detail(id).await?))
}
