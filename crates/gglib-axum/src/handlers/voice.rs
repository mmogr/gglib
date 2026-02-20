//! Axum handlers for the `/api/voice/*` endpoints.
//!
//! Handlers are thin wrappers — each calls exactly one `GuiBackend` method
//! and returns the result as JSON.  Request deserialization structs are
//! co-located here rather than in a separate types file to keep the handler
//! surface self-contained.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;

use gglib_core::ports::{AudioDeviceDto, VoiceModelsDto, VoiceStatusDto};

use crate::error::HttpError;
use crate::state::AppState;

// ── Request body shapes ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSttRequest {
    pub model_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetModeRequest {
    pub mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetVoiceRequest {
    pub voice_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSpeedRequest {
    pub speed: f32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAutoSpeakRequest {
    pub auto_speak: bool,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// `GET /api/voice/status`
pub async fn status(
    State(state): State<AppState>,
) -> Result<Json<VoiceStatusDto>, HttpError> {
    Ok(Json(state.gui.voice_status().await?))
}

/// `GET /api/voice/models`
pub async fn list_models(
    State(state): State<AppState>,
) -> Result<Json<VoiceModelsDto>, HttpError> {
    Ok(Json(state.gui.voice_list_models().await?))
}

/// `POST /api/voice/models/stt/{id}/download`
pub async fn download_stt_model(
    State(state): State<AppState>,
    Path(model_id): Path<String>,
) -> Result<Json<()>, HttpError> {
    state.gui.voice_download_stt_model(&model_id).await?;
    Ok(Json(()))
}

/// `POST /api/voice/models/tts/download`
pub async fn download_tts_model(
    State(state): State<AppState>,
) -> Result<Json<()>, HttpError> {
    state.gui.voice_download_tts_model().await?;
    Ok(Json(()))
}

/// `POST /api/voice/models/vad/download`
pub async fn download_vad_model(
    State(state): State<AppState>,
) -> Result<Json<()>, HttpError> {
    state.gui.voice_download_vad_model().await?;
    Ok(Json(()))
}

/// `POST /api/voice/stt/load`
pub async fn load_stt(
    State(state): State<AppState>,
    Json(req): Json<LoadSttRequest>,
) -> Result<Json<()>, HttpError> {
    state.gui.voice_load_stt(&req.model_id).await?;
    Ok(Json(()))
}

/// `POST /api/voice/tts/load`
pub async fn load_tts(
    State(state): State<AppState>,
) -> Result<Json<()>, HttpError> {
    state.gui.voice_load_tts().await?;
    Ok(Json(()))
}

/// `PUT /api/voice/mode`
pub async fn set_mode(
    State(state): State<AppState>,
    Json(req): Json<SetModeRequest>,
) -> Result<Json<()>, HttpError> {
    state.gui.voice_set_mode(&req.mode).await?;
    Ok(Json(()))
}

/// `PUT /api/voice/voice`
pub async fn set_voice(
    State(state): State<AppState>,
    Json(req): Json<SetVoiceRequest>,
) -> Result<Json<()>, HttpError> {
    state.gui.voice_set_voice(&req.voice_id).await?;
    Ok(Json(()))
}

/// `PUT /api/voice/speed`
pub async fn set_speed(
    State(state): State<AppState>,
    Json(req): Json<SetSpeedRequest>,
) -> Result<Json<()>, HttpError> {
    state.gui.voice_set_speed(req.speed).await?;
    Ok(Json(()))
}

/// `PUT /api/voice/auto-speak`
pub async fn set_auto_speak(
    State(state): State<AppState>,
    Json(req): Json<SetAutoSpeakRequest>,
) -> Result<Json<()>, HttpError> {
    state.gui.voice_set_auto_speak(req.auto_speak).await?;
    Ok(Json(()))
}

/// `POST /api/voice/unload`
pub async fn unload(
    State(state): State<AppState>,
) -> Result<Json<()>, HttpError> {
    state.gui.voice_unload().await?;
    Ok(Json(()))
}

/// `GET /api/voice/devices`
pub async fn list_devices(
    State(state): State<AppState>,
) -> Result<Json<Vec<AudioDeviceDto>>, HttpError> {
    Ok(Json(state.gui.voice_list_devices().await?))
}
