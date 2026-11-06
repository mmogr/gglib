//! Settings handlers - application configuration.

use axum::Json;
use axum::extract::State;

use crate::error::HttpError;
use crate::state::AppState;
use gglib_app_services::types::{
    AppSettings, InstalledTemplates, ModelsDirectoryInfo, UpdateSettingsRequest,
};

/// Get application settings.
pub(crate) async fn get(State(state): State<AppState>) -> Result<Json<AppSettings>, HttpError> {
    Ok(Json(state.settings.get().await?))
}

/// Update application settings.
pub(crate) async fn update(
    State(state): State<AppState>,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<AppSettings>, HttpError> {
    Ok(Json(state.settings.update(req).await?))
}

/// Add the starter profiles to the stored list. A stored profile that has
/// one's name is kept as it is, and named in the answer's `kept`.
pub(crate) async fn install_profile_templates(
    State(state): State<AppState>,
) -> Result<Json<InstalledTemplates>, HttpError> {
    Ok(Json(state.settings.install_profile_templates().await?))
}

/// Get models directory information.
pub(crate) async fn models_directory(
    State(state): State<AppState>,
) -> Result<Json<ModelsDirectoryInfo>, HttpError> {
    Ok(Json(state.settings.get_models_directory_info()?))
}

/// Update request for models directory.
#[derive(serde::Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct UpdateModelsDirectoryRequest {
    pub path: String,
}

/// Update models directory.
pub(crate) async fn update_models_directory(
    State(state): State<AppState>,
    Json(req): Json<UpdateModelsDirectoryRequest>,
) -> Result<Json<ModelsDirectoryInfo>, HttpError> {
    Ok(Json(state.settings.update_models_directory(&req.path)?))
}
