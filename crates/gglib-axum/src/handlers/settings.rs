//! Settings handlers - application configuration.

use axum::Json;
use axum::extract::State;

use crate::dto::SystemMemoryInfoDto;
use crate::error::HttpError;
use crate::state::AppState;
use gglib_gui::types::{AppSettings, ModelsDirectoryInfo, UpdateSettingsRequest};

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
///
/// Returns null if memory information cannot be determined (probe failed,
/// insufficient permissions, or suspiciously low values). Clients should
/// treat null as "unknown" rather than an error.
pub async fn memory(
    State(state): State<AppState>,
) -> Result<Json<Option<SystemMemoryInfoDto>>, HttpError> {
    let mem_opt = state.gui.get_system_memory()?;
    Ok(Json(mem_opt.map(SystemMemoryInfoDto::from)))
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
