//! Model handlers - CRUD operations for local models.

use axum::Json;
use axum::extract::{Path, State};

use crate::error::HttpError;
use crate::state::AppState;
use gglib_core::ModelFilterOptions;
use gglib_gui::types::{AddModelRequest, GuiModel, RemoveModelRequest, UpdateModelRequest};

/// List all models.
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<GuiModel>>, HttpError> {
    Ok(Json(state.gui.list_models().await?))
}

/// Get a single model by ID.
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<GuiModel>, HttpError> {
    Ok(Json(state.gui.get_model(id).await?))
}

/// Add a new model from a local file.
pub async fn add(
    State(state): State<AppState>,
    Json(req): Json<AddModelRequest>,
) -> Result<Json<GuiModel>, HttpError> {
    Ok(Json(state.gui.add_model(req).await?))
}

/// Update an existing model.
pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateModelRequest>,
) -> Result<Json<GuiModel>, HttpError> {
    Ok(Json(state.gui.update_model(id, req).await?))
}

/// Remove a model.
pub async fn remove(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<RemoveModelRequest>,
) -> Result<Json<String>, HttpError> {
    Ok(Json(state.gui.remove_model(id, req).await?))
}

/// Get all unique tags.
pub async fn list_tags(State(state): State<AppState>) -> Result<Json<Vec<String>>, HttpError> {
    Ok(Json(state.gui.list_tags().await?))
}

/// Get tags for a specific model.
pub async fn get_model_tags(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<String>>, HttpError> {
    Ok(Json(state.gui.get_model_tags(id).await?))
}

/// Add a tag to a model.
pub async fn add_tag(
    State(state): State<AppState>,
    Path((id, tag)): Path<(i64, String)>,
) -> Result<(), HttpError> {
    state.gui.add_model_tag(id, tag).await?;
    Ok(())
}

/// Request body for adding a tag via POST to /api/models/:id/tags
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
    state.gui.add_model_tag(id, request.tag).await?;
    Ok(())
}

/// Remove a tag from a model.
pub async fn remove_tag(
    State(state): State<AppState>,
    Path((id, tag)): Path<(i64, String)>,
) -> Result<(), HttpError> {
    state.gui.remove_model_tag(id, tag).await?;
    Ok(())
}

/// Get models with a specific tag.
pub async fn get_by_tag(
    State(state): State<AppState>,
    Path(tag): Path<String>,
) -> Result<Json<Vec<i64>>, HttpError> {
    Ok(Json(state.gui.get_models_by_tag(tag).await?))
}

/// Get filter options for the model list UI.
pub async fn filter_options(
    State(state): State<AppState>,
) -> Result<Json<ModelFilterOptions>, HttpError> {
    Ok(Json(state.gui.get_model_filter_options().await?))
}
