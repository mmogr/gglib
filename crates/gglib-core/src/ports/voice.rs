//! Voice pipeline port — trait abstraction for voice data & config operations.
//!
//! # Design Rules
//!
//! - DTOs here are transport-agnostic wire shapes (no `gglib-voice` types).
//! - Conversion from `gglib-voice` native types happens inside `gglib-voice`,
//!   never here. This keeps `gglib-core` free of any dependency on `gglib-voice`.
//! - `VoicePipelinePort` is the only surface `gglib-gui` and `gglib-axum`
//!   need in order to serve all 13 voice data/config endpoints.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── DTOs ─────────────────────────────────────────────────────────────────────

/// Current state of the voice pipeline.
// Wire-shape DTO: the four bools represent distinct pipeline state flags
// (is_active, stt_loaded, tts_loaded, auto_speak) that have clear, independent
// meanings. There is no sensible state-machine or enum grouping that would
// improve clarity for callers reading the JSON payload.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatusDto {
    /// Whether the pipeline is actively capturing/processing audio.
    pub is_active: bool,
    /// State machine label (e.g. `"idle"`, `"listening"`, `"recording"`).
    pub state: String,
    /// Interaction mode label (`"ptt"` or `"vad"`).
    pub mode: String,
    /// Whether an STT engine is loaded.
    pub stt_loaded: bool,
    /// Whether a TTS engine is loaded.
    pub tts_loaded: bool,
    /// ID of the currently loaded STT model, if any.
    pub stt_model_id: Option<String>,
    /// Currently selected TTS voice, if loaded.
    pub tts_voice: Option<String>,
    /// Whether LLM responses are spoken automatically.
    pub auto_speak: bool,
}

/// Information about a single STT model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SttModelInfoDto {
    /// Model identifier (e.g. `"base.en"`).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Download size in bytes.
    pub size_bytes: u64,
    /// Human-readable size string.
    pub size_display: String,
    /// Whether this model is English-only.
    pub english_only: bool,
    /// Quality rating (1–5).
    pub quality: u8,
    /// Relative speed rating (1 = fastest).
    pub speed: u8,
    /// Whether this is the recommended default model.
    pub is_default: bool,
    /// Whether the model archive is already present on disk.
    pub is_downloaded: bool,
}

/// Information about the TTS model bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsModelInfoDto {
    /// Model identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Download size in bytes.
    pub size_bytes: u64,
    /// Human-readable size string.
    pub size_display: String,
    /// Number of available voices in this bundle.
    pub voice_count: u32,
    /// Whether the model archive is already present on disk.
    pub is_downloaded: bool,
}

/// Information about a single TTS voice.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInfoDto {
    /// Voice identifier used in API calls.
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// Language/accent category.
    pub category: String,
}

/// Aggregated voice model catalog: STT list, TTS bundle, and VAD status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceModelsDto {
    /// All known STT models with download status.
    pub stt_models: Vec<SttModelInfoDto>,
    /// The single TTS model bundle with download status.
    pub tts_model: TtsModelInfoDto,
    /// Whether the Silero VAD model is downloaded.
    pub vad_downloaded: bool,
    /// Available TTS voices (populated when TTS model is loaded).
    pub voices: Vec<VoiceInfoDto>,
}

/// Information about an audio input device visible to the OS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceDto {
    /// Human-readable device name.
    pub name: String,
    /// Whether this is the system default input device.
    pub is_default: bool,
}

// ── Error ─────────────────────────────────────────────────────────────────────

/// Errors returned by `VoicePipelinePort` operations.
///
/// These map deterministically to `GuiError` variants, which in turn map to
/// HTTP status codes via the existing `From<GuiError> for HttpError` impl.
#[derive(Debug, Error)]
pub enum VoicePortError {
    /// The voice pipeline has not been initialised yet (no model loaded).
    #[error("Voice pipeline not initialised — load an STT or TTS model first")]
    NotInitialised,

    /// The pipeline is already in an active streaming state.
    #[error("Voice pipeline is already active")]
    AlreadyActive,

    /// The pipeline is initialised (models loaded) but has not been started.
    ///
    /// The caller should POST to `/api/voice/start` before calling audio I/O
    /// operations.  Maps to HTTP 409 Conflict.
    #[error("Voice pipeline is not active — call /api/voice/start first")]
    NotActive,

    /// A requested resource (model, device) was not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// A model failed to load (model file corrupt, incompatible format, etc.).
    #[error("Load error: {0}")]
    LoadError(String),

    /// A model download failed (network, disk, archive extraction).
    #[error("Download error: {0}")]
    DownloadError(String),

    /// Unexpected internal error.
    #[error("Internal voice error: {0}")]
    Internal(String),
}

// ── Port trait ────────────────────────────────────────────────────────────────

/// Port trait for voice data, configuration, and audio I/O operations.
///
/// Implemented by `VoiceService` in `gglib-voice`.
/// Consumed by `VoiceOps` in `gglib-gui` and delegated to by Axum handlers.
///
/// # Scope
///
/// This trait covers all 19 voice operations:
/// - **13 data/config** endpoints (no audio hardware required, curl-testable)
/// - **6 audio I/O** endpoints (`start`, `stop`, `ptt-start`, `ptt-stop`,
///   `speak`, `stop-speaking`)
#[async_trait]
pub trait VoicePipelinePort: Send + Sync {
    /// Return the current pipeline status (state machine, loaded models, etc.).
    async fn status(&self) -> Result<VoiceStatusDto, VoicePortError>;

    /// Return the full model catalog with per-model download status.
    async fn list_models(&self) -> Result<VoiceModelsDto, VoicePortError>;

    /// Download an STT model archive by ID (e.g. `"base.en"`).
    async fn download_stt_model(&self, model_id: &str) -> Result<(), VoicePortError>;

    /// Download the TTS model archive.
    async fn download_tts_model(&self) -> Result<(), VoicePortError>;

    /// Download the Silero VAD model.
    async fn download_vad_model(&self) -> Result<(), VoicePortError>;

    /// Load a downloaded STT model into the pipeline by ID.
    async fn load_stt(&self, model_id: &str) -> Result<(), VoicePortError>;

    /// Load the downloaded TTS model into the pipeline.
    async fn load_tts(&self) -> Result<(), VoicePortError>;

    /// Set the interaction mode (`"ptt"` | `"vad"`).
    async fn set_mode(&self, mode: &str) -> Result<(), VoicePortError>;

    /// Set the TTS voice by ID.
    async fn set_voice(&self, voice_id: &str) -> Result<(), VoicePortError>;

    /// Set the TTS playback speed (1.0 = normal).
    async fn set_speed(&self, speed: f32) -> Result<(), VoicePortError>;

    /// Enable or disable automatic TTS for LLM responses.
    async fn set_auto_speak(&self, enabled: bool) -> Result<(), VoicePortError>;

    /// Stop audio I/O and release all model memory.
    async fn unload(&self) -> Result<(), VoicePortError>;

    /// List available audio input devices.
    async fn list_devices(&self) -> Result<Vec<AudioDeviceDto>, VoicePortError>;

    // ── Audio I/O (Phase 3 / PR 2) ────────────────────────────────────────────

    /// Start the voice pipeline audio I/O.
    ///
    /// `mode` overrides the current interaction mode for this session
    /// (`"ptt"` | `"vad"`).  When `None`, the previously configured mode
    /// is used.
    ///
    /// Returns [`VoicePortError::NotInitialised`] if no STT model is loaded.
    /// Returns [`VoicePortError::AlreadyActive`] if the pipeline is already
    /// running.
    async fn start(&self, mode: Option<String>) -> Result<(), VoicePortError>;

    /// Stop audio I/O, releasing mic + playback resources, but keep STT/TTS
    /// models warm so the user can restart without a reload delay.
    async fn stop(&self) -> Result<(), VoicePortError>;

    /// Begin PTT recording (user pressed the talk button).
    ///
    /// Returns [`VoicePortError::NotInitialised`] if the pipeline is not
    /// active.
    async fn ptt_start(&self) -> Result<(), VoicePortError>;

    /// End PTT recording and transcribe the captured audio.
    ///
    /// Returns the transcript text (empty string if no speech was detected).
    async fn ptt_stop(&self) -> Result<String, VoicePortError>;

    /// Synthesize `text` via TTS and stream the audio to the speaker.
    ///
    /// This is an asynchronous operation: implementations may perform
    /// synthesis and playback work while this future is pending.  Callers
    /// MUST NOT assume that it returns immediately after dispatch.
    /// `VoiceEvent::SpeakingStarted` / `SpeakingFinished` are emitted via
    /// the SSE event bus to report speaking lifecycle events.
    async fn speak(&self, text: &str) -> Result<(), VoicePortError>;

    /// Interrupt any active TTS playback immediately.
    async fn stop_speaking(&self) -> Result<(), VoicePortError>;
}
