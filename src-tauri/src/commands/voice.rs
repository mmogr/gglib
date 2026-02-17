//! Voice mode Tauri commands.
//!
//! OS-specific commands for voice mode — microphone access, audio playback,
//! and voice pipeline management require native OS APIs that cannot go
//! through the HTTP transport.

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tracing::{error, info};

use gglib_voice::capture::AudioDeviceInfo;
use gglib_voice::models::{self, SttModelInfo, TtsModelInfo, VoiceModelCatalog};
use gglib_voice::pipeline::{
    VoiceEvent, VoiceInteractionMode, VoicePipeline, VoicePipelineConfig, VoiceState,
};
use gglib_voice::tts::{TtsEngine, VoiceInfo};

use crate::app::state::AppState;

// ── Event names ────────────────────────────────────────────────────

pub mod event_names {
    pub const VOICE_STATE_CHANGED: &str = "voice:state-changed";
    pub const VOICE_TRANSCRIPT: &str = "voice:transcript";
    pub const VOICE_SPEAKING_STARTED: &str = "voice:speaking-started";
    pub const VOICE_SPEAKING_FINISHED: &str = "voice:speaking-finished";
    pub const VOICE_AUDIO_LEVEL: &str = "voice:audio-level";
    pub const VOICE_ERROR: &str = "voice:error";
    pub const VOICE_MODEL_DOWNLOAD_PROGRESS: &str = "voice:model-download-progress";
}

// ── Event payloads ─────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatePayload {
    pub state: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceTranscriptPayload {
    pub text: String,
    pub is_final: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceAudioLevelPayload {
    pub level: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceErrorPayload {
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadProgressPayload {
    pub model_id: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub percent: f64,
}

// ── Status / info types ────────────────────────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatusResponse {
    pub is_active: bool,
    pub state: String,
    pub mode: String,
    pub stt_loaded: bool,
    pub tts_loaded: bool,
    pub stt_model_id: Option<String>,
    pub tts_voice: Option<String>,
    pub auto_speak: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceModelsResponse {
    pub stt_models: Vec<SttModelInfo>,
    pub stt_downloaded: Vec<String>,
    pub tts_model: TtsModelInfo,
    pub tts_downloaded: bool,
    pub voices: Vec<VoiceInfo>,
}

// ── Helper ─────────────────────────────────────────────────────────

fn emit_or_log<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(e) = app.emit(event, payload) {
        error!(error = %e, event, "Failed to emit voice event");
    }
}

fn voice_state_string(state: VoiceState) -> String {
    match state {
        VoiceState::Idle => "idle",
        VoiceState::Listening => "listening",
        VoiceState::Recording => "recording",
        VoiceState::Transcribing => "transcribing",
        VoiceState::Thinking => "thinking",
        VoiceState::Speaking => "speaking",
        VoiceState::Error => "error",
    }
    .to_string()
}

fn mode_string(mode: VoiceInteractionMode) -> String {
    match mode {
        VoiceInteractionMode::PushToTalk => "ptt",
        VoiceInteractionMode::VoiceActivityDetection => "vad",
    }
    .to_string()
}

// ── Event forwarding ───────────────────────────────────────────────

/// Spawn a background task that forwards VoicePipeline events to the Tauri frontend.
fn spawn_event_forwarder(
    app: AppHandle,
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<VoiceEvent>,
) {
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                VoiceEvent::StateChanged(state) => {
                    emit_or_log(
                        &app,
                        event_names::VOICE_STATE_CHANGED,
                        VoiceStatePayload {
                            state: voice_state_string(state),
                        },
                    );
                }
                VoiceEvent::Transcript { text, is_final } => {
                    emit_or_log(
                        &app,
                        event_names::VOICE_TRANSCRIPT,
                        VoiceTranscriptPayload { text, is_final },
                    );
                }
                VoiceEvent::SpeakingStarted => {
                    emit_or_log(&app, event_names::VOICE_SPEAKING_STARTED, ());
                }
                VoiceEvent::SpeakingFinished => {
                    emit_or_log(&app, event_names::VOICE_SPEAKING_FINISHED, ());
                }
                VoiceEvent::AudioLevel(level) => {
                    emit_or_log(
                        &app,
                        event_names::VOICE_AUDIO_LEVEL,
                        VoiceAudioLevelPayload { level },
                    );
                }
                VoiceEvent::Error(msg) => {
                    emit_or_log(
                        &app,
                        event_names::VOICE_ERROR,
                        VoiceErrorPayload { message: msg },
                    );
                }
            }
        }
    });
}

// ── Commands: Pipeline lifecycle ───────────────────────────────────

/// Start the voice pipeline.
///
/// If a pipeline already exists (e.g. from model preloading in settings),
/// it will be reused — only audio I/O is started. Otherwise a new pipeline
/// is created.
#[tauri::command]
pub async fn voice_start(
    mode: Option<String>,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let interaction_mode = match mode.as_deref() {
        Some("vad") => VoiceInteractionMode::VoiceActivityDetection,
        _ => VoiceInteractionMode::PushToTalk,
    };

    let mut voice = state.voice_pipeline.write().await;

    if let Some(ref mut pipeline) = *voice {
        // Pipeline already exists (models may be preloaded) — just start audio
        if !pipeline.is_active() {
            pipeline.set_mode(interaction_mode);
            pipeline.start().map_err(|e| format!("{e}"))?;
            info!(mode = ?interaction_mode, "Voice pipeline started (reused existing)");
        }
    } else {
        // Create fresh pipeline
        let config = VoicePipelineConfig {
            mode: interaction_mode,
            ..VoicePipelineConfig::default()
        };

        let (mut pipeline, event_rx) = VoicePipeline::new(config);
        pipeline.start().map_err(|e| format!("{e}"))?;
        spawn_event_forwarder(app, event_rx);
        *voice = Some(pipeline);
        info!(mode = ?interaction_mode, "Voice pipeline started (new)");
    }

    Ok(())
}

/// Stop the voice pipeline.
#[tauri::command]
pub async fn voice_stop(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut voice = state.voice_pipeline.write().await;
    if let Some(ref mut pipeline) = *voice {
        pipeline.stop();
    }
    *voice = None;

    info!("Voice pipeline stopped");
    Ok(())
}

/// Get current voice pipeline status.
#[tauri::command]
pub async fn voice_status(
    state: tauri::State<'_, AppState>,
) -> Result<VoiceStatusResponse, String> {
    let voice = state.voice_pipeline.read().await;
    match voice.as_ref() {
        Some(pipeline) => Ok(VoiceStatusResponse {
            is_active: pipeline.is_active(),
            state: voice_state_string(pipeline.state()),
            mode: mode_string(pipeline.mode()),
            stt_loaded: pipeline.is_stt_loaded(),
            tts_loaded: pipeline.is_tts_loaded(),
            stt_model_id: None, // TODO: track in pipeline
            tts_voice: None,    // TODO: track in pipeline
            auto_speak: pipeline.auto_speak(),
        }),
        None => Ok(VoiceStatusResponse {
            is_active: false,
            state: "idle".to_string(),
            mode: "ptt".to_string(),
            stt_loaded: false,
            tts_loaded: false,
            stt_model_id: None,
            tts_voice: None,
            auto_speak: true,
        }),
    }
}

// ── Commands: Push-to-Talk ─────────────────────────────────────────

/// Begin PTT recording (user pressed talk button).
#[tauri::command]
pub async fn voice_ptt_start(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut voice = state.voice_pipeline.write().await;
    let pipeline = voice.as_mut().ok_or("Voice pipeline not active")?;
    pipeline.ptt_start().map_err(|e| format!("{e}"))
}

/// End PTT recording and transcribe (user released talk button).
///
/// Returns the transcribed text.
#[tauri::command]
pub async fn voice_ptt_stop(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let mut voice = state.voice_pipeline.write().await;
    let pipeline = voice.as_mut().ok_or("Voice pipeline not active")?;
    pipeline.ptt_stop().map_err(|e| format!("{e}"))
}

// ── Commands: TTS ──────────────────────────────────────────────────

/// Speak text through TTS.
#[tauri::command]
pub async fn voice_speak(text: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut voice = state.voice_pipeline.write().await;
    let pipeline = voice.as_mut().ok_or("Voice pipeline not active")?;
    pipeline.speak(&text).await.map_err(|e| format!("{e}"))
}

/// Stop active TTS playback.
#[tauri::command]
pub async fn voice_stop_speaking(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut voice = state.voice_pipeline.write().await;
    if let Some(ref mut pipeline) = *voice {
        pipeline.stop_speaking();
    }
    Ok(())
}

// ── Commands: Model management ─────────────────────────────────────

/// List available voice models and their download status.
#[tauri::command]
pub async fn voice_list_models() -> Result<VoiceModelsResponse, String> {
    let stt_models = VoiceModelCatalog::stt_models();
    let downloaded = VoiceModelCatalog::downloaded_stt_models().map_err(|e| format!("{e}"))?;
    let downloaded_ids: Vec<String> = downloaded.iter().map(|m| m.id.0.clone()).collect();

    let tts_model = VoiceModelCatalog::tts_model();
    let tts_downloaded = VoiceModelCatalog::is_tts_downloaded().unwrap_or(false);

    let voices = TtsEngine::available_voices();

    Ok(VoiceModelsResponse {
        stt_models,
        stt_downloaded: downloaded_ids,
        tts_model,
        tts_downloaded,
        voices,
    })
}

/// Download an STT model.
#[tauri::command]
pub async fn voice_download_stt_model(model_id: String, app: AppHandle) -> Result<(), String> {
    let model_id_clone = model_id.clone();
    let app_clone = app.clone();

    let path = models::ensure_stt_model(&model_id, move |downloaded, total| {
        let percent = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        emit_or_log(
            &app_clone,
            event_names::VOICE_MODEL_DOWNLOAD_PROGRESS,
            ModelDownloadProgressPayload {
                model_id: model_id_clone.clone(),
                bytes_downloaded: downloaded,
                total_bytes: total,
                percent,
            },
        );
    })
    .await
    .map_err(|e| format!("{e}"))?;

    info!(model_id = %model_id, path = %path.display(), "STT model downloaded");
    Ok(())
}

/// Download the TTS model (Kokoro).
#[tauri::command]
pub async fn voice_download_tts_model(app: AppHandle) -> Result<(), String> {
    let app_clone = app.clone();

    let (model_path, voices_path) = models::ensure_tts_model(move |downloaded, total| {
        let percent = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        emit_or_log(
            &app_clone,
            event_names::VOICE_MODEL_DOWNLOAD_PROGRESS,
            ModelDownloadProgressPayload {
                model_id: "kokoro-v1.0".to_string(),
                bytes_downloaded: downloaded,
                total_bytes: total,
                percent,
            },
        );
    })
    .await
    .map_err(|e| format!("{e}"))?;

    info!(
        model = %model_path.display(),
        voices = %voices_path.display(),
        "TTS model downloaded"
    );
    Ok(())
}

/// Load an STT model into the pipeline (auto-creates an idle pipeline if needed).
#[tauri::command]
pub async fn voice_load_stt(
    model_id: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let model = VoiceModelCatalog::find_stt_model(&model_id)
        .ok_or_else(|| format!("Unknown STT model: {model_id}"))?;

    let path = VoiceModelCatalog::stt_model_path(&model).map_err(|e| format!("{e}"))?;

    if !path.exists() {
        return Err(format!("STT model not downloaded: {model_id}"));
    }

    let mut voice = state.voice_pipeline.write().await;
    if voice.is_none() {
        let config = VoicePipelineConfig::default();
        let (pipeline, event_rx) = VoicePipeline::new(config);
        spawn_event_forwarder(app, event_rx);
        *voice = Some(pipeline);
        info!("Created idle voice pipeline for model preloading");
    }
    let pipeline = voice.as_mut().unwrap();
    pipeline.load_stt(&path).map_err(|e| format!("{e}"))
}

/// Load the TTS model into the pipeline (auto-creates an idle pipeline if needed).
#[tauri::command]
pub async fn voice_load_tts(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // The TTS model directory contains model.onnx, voices.bin, tokens.txt, etc.
    let tts_dir = VoiceModelCatalog::voice_models_dir()
        .map(|d| d.join("tts"))
        .map_err(|e| format!("{e}"))?;

    if !tts_dir.exists() {
        return Err("TTS model not downloaded".to_string());
    }

    let mut voice = state.voice_pipeline.write().await;
    if voice.is_none() {
        let config = VoicePipelineConfig::default();
        let (pipeline, event_rx) = VoicePipeline::new(config);
        spawn_event_forwarder(app, event_rx);
        *voice = Some(pipeline);
        info!("Created idle voice pipeline for model preloading");
    }
    let pipeline = voice.as_mut().unwrap();
    pipeline
        .load_tts(&tts_dir)
        .await
        .map_err(|e| format!("{e}"))
}

// ── Commands: Configuration ────────────────────────────────────────

/// Set the interaction mode (PTT or VAD).
#[tauri::command]
pub async fn voice_set_mode(mode: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let interaction_mode = match mode.as_str() {
        "vad" => VoiceInteractionMode::VoiceActivityDetection,
        "ptt" => VoiceInteractionMode::PushToTalk,
        _ => return Err(format!("Unknown voice mode: {mode}")),
    };

    let mut voice = state.voice_pipeline.write().await;
    if let Some(ref mut pipeline) = *voice {
        pipeline.set_mode(interaction_mode);
    }
    Ok(())
}

/// Set the TTS voice.
#[tauri::command]
pub async fn voice_set_voice(
    voice_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut voice = state.voice_pipeline.write().await;
    if let Some(ref mut pipeline) = *voice {
        pipeline.set_voice(&voice_id);
    }
    Ok(())
}

/// Set the TTS playback speed.
#[tauri::command]
pub async fn voice_set_speed(speed: f32, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut voice = state.voice_pipeline.write().await;
    if let Some(ref mut pipeline) = *voice {
        pipeline.set_speed(speed);
    }
    Ok(())
}

/// Set whether LLM responses are automatically spoken.
#[tauri::command]
pub async fn voice_set_auto_speak(
    auto_speak: bool,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut voice = state.voice_pipeline.write().await;
    if let Some(ref mut pipeline) = *voice {
        pipeline.set_auto_speak(auto_speak);
    }
    Ok(())
}

// ── Commands: Device enumeration ───────────────────────────────────

/// List available audio input devices.
#[tauri::command]
pub fn voice_list_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    gglib_voice::capture::AudioCapture::list_devices().map_err(|e| format!("{e}"))
}
