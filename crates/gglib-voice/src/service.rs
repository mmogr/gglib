//! `VoiceService` — the adapter that implements `VoicePipelinePort`.
//!
//! This module is the single place where `gglib-voice` native types are
//! converted to the transport-agnostic DTOs defined in `gglib-core`.
//! Nothing outside this file should import `SttModelInfo`, `TtsModelInfo`, etc.
//!
//! # Locking discipline
//!
//! All mutations use `pipeline.write().await`; all read-only queries use
//! `pipeline.read().await`.  Download operations hold **no pipeline lock**
//! while streaming over the network — the lock is only held for the brief
//! file-existence check before the download, and for `load_stt`/`load_tts`
//! after the download completes.  This prevents blocking the executor and
//! prevents any deadlock with the event emitter.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::info;

use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use gglib_core::ports::voice::{
    AudioDeviceDto, SttModelInfoDto, TtsModelInfoDto, VoiceInfoDto, VoiceModelsDto,
    VoicePipelinePort, VoicePortError, VoiceStatusDto,
};

use crate::capture::AudioCapture;
use crate::models::{self, VoiceModelCatalog};
use crate::pipeline::{VoiceInteractionMode, VoicePipeline, VoicePipelineConfig, VoiceState};
use crate::tts::TtsEngine;

// ── Pending config ────────────────────────────────────────────────────────────

/// User-visible config settings that persist across pipeline load/unload cycles.
///
/// Written by `set_mode`, `set_voice`, `set_speed`, `set_auto_speak` and read
/// back by `status()` when no pipeline is loaded.  Applied to a freshly
/// created pipeline at the end of `load_stt` / `load_tts`.
struct PendingConfig {
    mode: VoiceInteractionMode,
    voice_id: Option<String>,
    speed: f32,
    auto_speak: bool,
}

impl Default for PendingConfig {
    fn default() -> Self {
        Self {
            mode: VoiceInteractionMode::PushToTalk,
            voice_id: None,
            speed: 1.0,
            auto_speak: true,
        }
    }
}

// ── Service struct ────────────────────────────────────────────────────────────

/// Implements [`VoicePipelinePort`] by wrapping the shared pipeline state.
///
/// The `Arc<RwLock<_>>` is shared with any Tauri audio commands that are not
/// yet migrated to HTTP; they access the same pipeline via [`Self::pipeline()`].
pub struct VoiceService {
    pipeline: Arc<RwLock<Option<VoicePipeline>>>,
    emitter: Arc<dyn AppEventEmitter>,
    /// Config persisted even when pipeline is None.
    /// Uses a std (non-async) lock because it is only accessed in sync
    /// context — never across an `.await` point.
    config: std::sync::RwLock<PendingConfig>,
}

impl VoiceService {
    /// Create a new service with no pipeline loaded.
    ///
    /// The pipeline starts as `None` and is populated lazily when
    /// `load_stt` or `load_tts` is first called.
    pub fn new(emitter: Arc<dyn AppEventEmitter>) -> Self {
        Self {
            pipeline: Arc::new(RwLock::new(None)),
            emitter,
            config: std::sync::RwLock::new(PendingConfig::default()),
        }
    }

    /// Create a service that shares an existing pipeline `Arc`.
    ///
    /// Used in Tauri bootstrap so that the HTTP layer and the remaining
    /// Tauri audio commands share the same `VoicePipeline` instance.
    pub fn from_arc(
        pipeline: Arc<RwLock<Option<VoicePipeline>>>,
        emitter: Arc<dyn AppEventEmitter>,
    ) -> Self {
        Self {
            pipeline,
            emitter,
            config: std::sync::RwLock::new(PendingConfig::default()),
        }
    }

    /// Return a clone of the underlying pipeline `Arc`.
    ///
    /// Tauri audio commands (`voice_start`, `voice_stop`, `voice_ptt_start`,
    /// `voice_ptt_stop`, `voice_speak`, `voice_stop_speaking`) call this to
    /// obtain the same shared lock they previously accessed via
    /// `AppState.voice_pipeline`.
    pub fn pipeline(&self) -> Arc<RwLock<Option<VoicePipeline>>> {
        Arc::clone(&self.pipeline)
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Convert a `VoiceError` into its closest `VoicePortError` equivalent.
///
/// This conversion lives here, in `gglib-voice`, so that `gglib-core` never
/// needs to import `gglib-voice`.  The dependency arrow stays one-way.
fn to_port_err(e: crate::error::VoiceError) -> VoicePortError {
    use crate::error::VoiceError;
    match e {
        VoiceError::ModelNotFound(p) => VoicePortError::NotFound(p.display().to_string()),
        VoiceError::AlreadyActive => VoicePortError::AlreadyActive,
        VoiceError::ModelLoadError(s) => VoicePortError::LoadError(s),
        VoiceError::DownloadError { name, source } => {
            VoicePortError::DownloadError(format!("{name}: {source}"))
        }
        other => VoicePortError::Internal(other.to_string()),
    }
}

fn state_label(s: VoiceState) -> String {
    match s {
        VoiceState::Idle => "idle",
        VoiceState::Listening => "listening",
        VoiceState::Recording => "recording",
        VoiceState::Transcribing => "transcribing",
        VoiceState::Thinking => "thinking",
        VoiceState::Speaking => "speaking",
        VoiceState::Error => "error",
    }
    .to_owned()
}

fn mode_label(m: VoiceInteractionMode) -> String {
    match m {
        VoiceInteractionMode::PushToTalk => "ptt",
        VoiceInteractionMode::VoiceActivityDetection => "vad",
    }
    .to_owned()
}

// ── VoicePipelinePort implementation ─────────────────────────────────────────

#[async_trait]
impl VoicePipelinePort for VoiceService {
    async fn status(&self) -> Result<VoiceStatusDto, VoicePortError> {
        // Shared read lock — does not block other concurrent reads.
        let guard = self.pipeline.read().await;
        let dto = guard.as_ref().map_or_else(
            || {
                // No pipeline yet — return defaults from PendingConfig so that
                // settings written via set_mode / set_auto_speak / etc. are
                // visible before any model is loaded.
                let cfg = self.config.read().unwrap();
                VoiceStatusDto {
                    is_active: false,
                    state: "idle".to_owned(),
                    mode: mode_label(cfg.mode),
                    stt_loaded: false,
                    tts_loaded: false,
                    stt_model_id: None,
                    tts_voice: cfg.voice_id.clone(),
                    auto_speak: cfg.auto_speak,
                }
            },
            |p| VoiceStatusDto {
                is_active: p.is_active(),
                state: state_label(p.state()),
                mode: mode_label(p.mode()),
                stt_loaded: p.is_stt_loaded(),
                tts_loaded: p.is_tts_loaded(),
                stt_model_id: p.stt_model_id().map(str::to_owned),
                tts_voice: Some(p.tts_voice().to_owned()),
                auto_speak: p.auto_speak(),
            },
        );
        // Release the read lock before returning.
        drop(guard);
        Ok(dto)
    }

    async fn list_models(&self) -> Result<VoiceModelsDto, VoicePortError> {
        // No pipeline lock needed — catalog is stateless.
        let all_stt = VoiceModelCatalog::stt_models();
        let downloaded_stt = VoiceModelCatalog::downloaded_stt_models().map_err(to_port_err)?;
        let downloaded_ids: std::collections::HashSet<String> =
            downloaded_stt.iter().map(|m| m.id.0.clone()).collect();

        let stt_models: Vec<SttModelInfoDto> = all_stt
            .into_iter()
            .map(|m| SttModelInfoDto {
                is_downloaded: downloaded_ids.contains(&m.id.0),
                id: m.id.0,
                name: m.name,
                size_bytes: m.size_bytes,
                size_display: m.size_display,
                english_only: m.english_only,
                quality: m.quality,
                speed: m.speed,
                is_default: m.is_default,
            })
            .collect();

        let tts = VoiceModelCatalog::tts_model();
        let tts_downloaded = VoiceModelCatalog::is_tts_downloaded().unwrap_or(false);
        let tts_model = TtsModelInfoDto {
            id: tts.id.0,
            name: tts.name,
            size_bytes: tts.size_bytes,
            size_display: tts.size_display,
            voice_count: tts.voice_count,
            is_downloaded: tts_downloaded,
        };

        let vad_downloaded = VoiceModelCatalog::is_vad_downloaded().unwrap_or(false);

        let voices: Vec<VoiceInfoDto> = TtsEngine::available_voices()
            .into_iter()
            .map(|v| VoiceInfoDto {
                id: v.id,
                name: v.name,
                category: v.category,
            })
            .collect();

        Ok(VoiceModelsDto {
            stt_models,
            tts_model,
            vad_downloaded,
            voices,
        })
    }

    #[allow(clippy::cast_precision_loss)] // progress % — sub-ulp precision not needed
    async fn download_stt_model(&self, model_id: &str) -> Result<(), VoicePortError> {
        // Clone the emitter and model_id before entering the download so
        // the closure is 'static and the pipeline lock is never held.
        let emitter = Arc::clone(&self.emitter);
        let model_id_owned = model_id.to_owned();

        let path = models::ensure_stt_model(model_id, move |downloaded, total| {
            let percent = if total > 0 {
                (downloaded as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            emitter.emit(AppEvent::VoiceModelDownloadProgress {
                model_id: model_id_owned.clone(),
                bytes_downloaded: downloaded,
                total_bytes: total,
                percent,
            });
        })
        .await
        .map_err(to_port_err)?;

        info!(model_id, path = %path.display(), "STT model downloaded via HTTP");
        Ok(())
    }

    #[allow(clippy::cast_precision_loss)] // progress % — sub-ulp precision not needed
    async fn download_tts_model(&self) -> Result<(), VoicePortError> {
        let emitter = Arc::clone(&self.emitter);
        let model_id = VoiceModelCatalog::tts_model().id.0;
        let model_id_clone = model_id.clone();

        let path = models::ensure_tts_model(move |downloaded, total| {
            let percent = if total > 0 {
                (downloaded as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            emitter.emit(AppEvent::VoiceModelDownloadProgress {
                model_id: model_id_clone.clone(),
                bytes_downloaded: downloaded,
                total_bytes: total,
                percent,
            });
        })
        .await
        .map_err(to_port_err)?;

        info!(model_id, path = %path.display(), "TTS model downloaded via HTTP");
        Ok(())
    }

    #[allow(clippy::cast_precision_loss)] // progress % — sub-ulp precision not needed
    async fn download_vad_model(&self) -> Result<(), VoicePortError> {
        let emitter = Arc::clone(&self.emitter);

        let path = models::ensure_vad_model(move |downloaded, total| {
            let percent = if total > 0 {
                (downloaded as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            emitter.emit(AppEvent::VoiceModelDownloadProgress {
                model_id: "silero-vad".to_owned(),
                bytes_downloaded: downloaded,
                total_bytes: total,
                percent,
            });
        })
        .await
        .map_err(to_port_err)?;

        info!(path = %path.display(), "VAD model downloaded via HTTP");
        Ok(())
    }

    // The write-lock guard must stay alive for the duration of the function
    // because `pipeline` borrows from it; early drop is not possible here.
    #[allow(clippy::significant_drop_tightening)]
    async fn load_stt(&self, model_id: &str) -> Result<(), VoicePortError> {
        // Resolve catalog + path *before* acquiring the write lock to
        // minimise lock hold time.
        let model = VoiceModelCatalog::find_stt_model(model_id)
            .ok_or_else(|| VoicePortError::NotFound(format!("Unknown STT model: {model_id}")))?;
        let path = VoiceModelCatalog::stt_model_path(&model).map_err(to_port_err)?;
        if !path.exists() {
            return Err(VoicePortError::NotFound(format!(
                "STT model not downloaded: {model_id}"
            )));
        }

        let mut guard = self.pipeline.write().await;
        if guard.is_none() {
            // Create an idle pipeline for model preloading.
            // The event channel receiver is dropped intentionally;
            // Phase 2 will wire it to the SSE broadcaster.
            let (pipeline, _event_rx) = VoicePipeline::new(VoicePipelineConfig::default());
            *guard = Some(pipeline);
            info!("Created idle voice pipeline for STT preloading");
        }
        let pipeline = guard.as_mut().expect("just set above");
        if pipeline.is_active() {
            pipeline.stop();
        }
        pipeline.load_stt(&path, model_id).map_err(to_port_err)?;

        // Apply any pending config that was written before the pipeline existed.
        {
            let cfg = self.config.read().unwrap();
            pipeline.set_mode(cfg.mode);
            if let Some(ref v) = cfg.voice_id {
                pipeline.set_voice(v);
            }
            pipeline.set_speed(cfg.speed);
            pipeline.set_auto_speak(cfg.auto_speak);
        }
        info!(model_id, "STT model loaded via HTTP");
        Ok(())
    }

    // The write-lock guard must stay alive for the duration of the function
    // because `pipeline` borrows from it; early drop is not possible here.
    #[allow(clippy::significant_drop_tightening)]
    async fn load_tts(&self) -> Result<(), VoicePortError> {
        let tts_dir = VoiceModelCatalog::tts_model_path().map_err(to_port_err)?;
        if !tts_dir.exists() {
            return Err(VoicePortError::NotFound(
                "TTS model not downloaded".to_owned(),
            ));
        }

        let mut guard = self.pipeline.write().await;
        if guard.is_none() {
            let (pipeline, _event_rx) = VoicePipeline::new(VoicePipelineConfig::default());
            *guard = Some(pipeline);
            info!("Created idle voice pipeline for TTS preloading");
        }
        let pipeline = guard.as_mut().expect("just set above");
        pipeline.load_tts(&tts_dir).await.map_err(to_port_err)?;

        // Apply any pending config that was written before the pipeline existed.
        {
            let cfg = self.config.read().unwrap();
            pipeline.set_mode(cfg.mode);
            if let Some(ref v) = cfg.voice_id {
                pipeline.set_voice(v);
            }
            pipeline.set_speed(cfg.speed);
            pipeline.set_auto_speak(cfg.auto_speak);
        }
        info!("TTS model loaded via HTTP");
        Ok(())
    }

    async fn set_mode(&self, mode: &str) -> Result<(), VoicePortError> {
        let interaction_mode = match mode {
            "vad" => VoiceInteractionMode::VoiceActivityDetection,
            "ptt" => VoiceInteractionMode::PushToTalk,
            other => {
                return Err(VoicePortError::NotFound(format!(
                    "Unknown voice mode: {other}"
                )));
            }
        };
        // Always persist — survives pipeline being None.
        self.config.write().unwrap().mode = interaction_mode;
        // Also apply to the live pipeline if one exists.
        let mut guard = self.pipeline.write().await;
        if let Some(ref mut p) = *guard {
            p.set_mode(interaction_mode);
        }
        drop(guard);
        Ok(())
    }

    async fn set_voice(&self, voice_id: &str) -> Result<(), VoicePortError> {
        self.config.write().unwrap().voice_id = Some(voice_id.to_owned());
        let mut guard = self.pipeline.write().await;
        if let Some(ref mut p) = *guard {
            p.set_voice(voice_id);
        }
        drop(guard);
        Ok(())
    }

    async fn set_speed(&self, speed: f32) -> Result<(), VoicePortError> {
        self.config.write().unwrap().speed = speed;
        let mut guard = self.pipeline.write().await;
        if let Some(ref mut p) = *guard {
            p.set_speed(speed);
        }
        drop(guard);
        Ok(())
    }

    async fn set_auto_speak(&self, enabled: bool) -> Result<(), VoicePortError> {
        self.config.write().unwrap().auto_speak = enabled;
        let mut guard = self.pipeline.write().await;
        if let Some(ref mut p) = *guard {
            p.set_auto_speak(enabled);
        }
        drop(guard);
        Ok(())
    }

    async fn unload(&self) -> Result<(), VoicePortError> {
        let mut guard = self.pipeline.write().await;
        if let Some(ref mut p) = *guard {
            if p.is_active() {
                p.stop();
            }
        }
        *guard = None;
        drop(guard);
        info!("Voice pipeline unloaded via HTTP");
        Ok(())
    }

    async fn list_devices(&self) -> Result<Vec<AudioDeviceDto>, VoicePortError> {
        // No pipeline lock needed — device enumeration queries the OS directly.
        let devices = AudioCapture::list_devices().map_err(to_port_err)?;
        let dtos = devices
            .into_iter()
            .map(|d| AudioDeviceDto {
                name: d.name,
                is_default: d.is_default,
            })
            .collect();
        Ok(dtos)
    }
}
