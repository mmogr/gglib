//! Settings handlers - application configuration.

use axum::Json;
use axum::extract::State;

use crate::error::HttpError;
use crate::routes::AppState;
use gglib_gui::types::{AppSettings, ModelsDirectoryInfo, SystemMemoryInfo, UpdateSettingsRequest};

/// Get application settings.
pub async fn get(State(state): State<AppState>) -> Result<Json<AppSettings>, HttpError> {
    Ok(Json(state.gui.get_settings().await?))
}

/// Update application settings.
pub async fn update(
    State(state): State<AppState>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<AppSettings>, HttpError> {
    Ok(Json(state.gui.update_settings(req).await?))
}

/// Get system memory information.
pub async fn memory(State(state): State<AppState>) -> Result<Json<SystemMemoryInfo>, HttpError> {
    Ok(Json(state.gui.get_system_memory()?))
}

/// Get models directory information.
pub async fn models_directory(
    State(state): State<AppState>,
) -> Result<Json<ModelsDirectoryInfo>, HttpError> {
    Ok(Json(state.gui.get_models_directory_info()?))
}

/// Update request for models directory.
#[derive(serde::Deserialize)]
pub struct UpdateModelsDirectoryRequest {
    pub path: String,
}

/// Update models directory.
pub async fn update_models_directory(
    State(state): State<AppState>,
    Json(req): Json<UpdateModelsDirectoryRequest>,
) -> Result<Json<ModelsDirectoryInfo>, HttpError> {
    Ok(Json(state.gui.update_models_directory(req.path)?))
}
