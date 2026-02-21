//! Voice mode Tauri commands.
//!
//! Retains only the 6 audio I/O commands that require direct OS audio hardware
//! access.  The 13 data/config operations (status, model listing/download/load,
//! configuration, device listing) are now served by the Axum HTTP API — see
//! `crates/gglib-axum/src/handlers/voice.rs`.

use tracing::info;

use gglib_voice::models::VoiceModelCatalog;
use gglib_voice::pipeline::{VoiceInteractionMode, VoicePipeline, VoicePipelineConfig};
use gglib_voice::service::spawn_event_bridge;

use crate::app::state::AppState;

// ── Commands: Pipeline lifecycle ───────────────────────────────────

/// Start the voice pipeline.
///
/// If a pipeline already exists (e.g. from model preloading in settings),
/// it will be reused — only audio I/O is started. Otherwise a new pipeline
/// is created.
#[tauri::command]
pub async fn voice_start(
    mode: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let interaction_mode = match mode.as_deref() {
        Some("vad") => VoiceInteractionMode::VoiceActivityDetection,
        _ => VoiceInteractionMode::PushToTalk,
    };

    let _pipeline = state.voice_service.pipeline();
    let mut voice = _pipeline.write().await;

    if let Some(ref mut pipeline) = *voice {
        // Pipeline already exists (models may be preloaded) — just start audio
        if !pipeline.is_active() {
            pipeline.set_mode(interaction_mode);
            pipeline.start().map_err(|e| format!("{e}"))?;
            info!(mode = ?interaction_mode, "Voice pipeline started (reused existing)");
        }
    } else {
        // Resolve VAD model path if Silero model is downloaded
        let vad_model_path = VoiceModelCatalog::vad_model_path()
            .ok()
            .filter(|p| p.exists());

        // Create fresh pipeline
        let config = VoicePipelineConfig {
            mode: interaction_mode,
            vad_model_path,
            ..VoicePipelineConfig::default()
        };

        let (mut pipeline, event_rx) = VoicePipeline::new(config);
        pipeline.start().map_err(|e| format!("{e}"))?;
        spawn_event_bridge(event_rx, state.voice_service.emitter());
        *voice = Some(pipeline);
        info!(mode = ?interaction_mode, "Voice pipeline started (new)");
    }

    Ok(())
}

/// Stop the voice pipeline, releasing audio resources (microphone + playback)
/// but keeping loaded STT/TTS models in memory.
///
/// This allows the user to toggle voice off and back on without the
/// 5–10 second model-reload delay. The pipeline transitions to `Idle` and
/// the OS microphone indicator turns off immediately.
///
/// To also free the model memory, call [`voice_unload`] instead.
#[tauri::command]
pub async fn voice_stop(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let _pipeline = state.voice_service.pipeline();
    let mut voice = _pipeline.write().await;
    if let Some(ref mut pipeline) = *voice {
        pipeline.stop();
    }
    // Intentionally keep *voice = Some(pipeline) so loaded models stay warm.

    info!("Voice pipeline stopped (models retained)");
    Ok(())
}

// ── Commands: Push-to-Talk ─────────────────────────────────────────

/// Begin PTT recording (user pressed talk button).
#[tauri::command]
pub async fn voice_ptt_start(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let _pipeline = state.voice_service.pipeline();
    let mut voice = _pipeline.write().await;
    let pipeline = voice.as_mut().ok_or("Voice pipeline not active")?;
    pipeline.ptt_start().map_err(|e| format!("{e}"))
}

/// End PTT recording and transcribe (user released talk button).
///
/// Returns the transcribed text.
#[tauri::command]
pub async fn voice_ptt_stop(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let _pipeline = state.voice_service.pipeline();
    let mut voice = _pipeline.write().await;
    let pipeline = voice.as_mut().ok_or("Voice pipeline not active")?;
    pipeline.ptt_stop().await.map_err(|e| format!("{e}"))
}

// ── Commands: TTS ──────────────────────────────────────────────────

/// Speak text through TTS.
#[tauri::command]
pub async fn voice_speak(text: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let _pipeline = state.voice_service.pipeline();
    let mut voice = _pipeline.write().await;
    let pipeline = voice.as_mut().ok_or("Voice pipeline not active")?;
    pipeline.speak(&text).await.map_err(|e| format!("{e}"))
}

/// Stop active TTS playback.
#[tauri::command]
pub async fn voice_stop_speaking(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let _pipeline = state.voice_service.pipeline();
    let mut voice = _pipeline.write().await;
    if let Some(ref mut pipeline) = *voice {
        pipeline.stop_speaking();
    }
    Ok(())
}
