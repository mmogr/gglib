//! Voice engine backend traits — engine-agnostic interfaces for STT and TTS.
//!
//! This module defines the [`SttBackend`] and [`TtsBackend`] traits that
//! abstract over concrete speech engine implementations (Whisper, Kokoro,
//! sherpa-onnx, etc.). The [`VoicePipeline`](crate::pipeline::VoicePipeline)
//! operates on trait objects (`Box<dyn SttBackend>`, `Box<dyn TtsBackend>`)
//! so that engines can be swapped without touching the pipeline logic.
//!
//! ## Backend implementations
//!
//! | Feature    | Module             | STT | TTS |
//! |------------|--------------------|-----|-----|
//! | `kokoro`   | [`kokoro`]         |     |  ✓  |
//! | `whisper`  | [`whisper`]        |  ✓  |     |

#[cfg(feature = "kokoro")]
pub mod kokoro;
#[cfg(feature = "whisper")]
pub mod whisper;

use std::time::Duration;

use crate::error::VoiceError;

// ── Shared types ───────────────────────────────────────────────────

/// Audio produced by TTS synthesis.
#[derive(Debug, Clone)]
pub struct TtsAudio {
    /// PCM f32 samples.
    pub samples: Vec<f32>,

    /// Sample rate of the audio (e.g., 24 000 Hz for Kokoro).
    pub sample_rate: u32,

    /// Duration of the audio.
    pub duration: Duration,
}

/// Information about an available TTS voice.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceInfo {
    /// Voice identifier (used in API calls).
    pub id: String,

    /// Human-readable display name.
    pub name: String,

    /// Language/accent category.
    pub category: String,

    /// Gender.
    pub gender: VoiceGender,
}

/// Voice gender.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VoiceGender {
    Female,
    Male,
}

// ── STT Backend Trait ──────────────────────────────────────────────

/// Backend-agnostic speech-to-text engine.
///
/// Implementations must be `Send + Sync` so the pipeline can hold them
/// across `.await` points behind a `tokio::sync::RwLock`.
pub trait SttBackend: Send + Sync {
    /// Transcribe audio samples to text.
    ///
    /// # Arguments
    /// * `audio` — PCM f32 samples at 16 kHz mono.
    ///
    /// # Returns
    /// The transcribed text, or an empty string if no speech was detected.
    fn transcribe(&self, audio: &[f32]) -> Result<String, VoiceError>;

    /// Transcribe audio with a segment callback for streaming partial results.
    ///
    /// The callback is invoked for each transcribed segment as it becomes
    /// available.
    fn transcribe_with_callback(
        &self,
        audio: &[f32],
        on_segment: Box<dyn FnMut(&str) + Send + 'static>,
    ) -> Result<String, VoiceError>;

    /// Get the configured language code (e.g., `"en"`, `"auto"`).
    fn language(&self) -> &str;
}

// ── TTS Backend Trait ──────────────────────────────────────────────

/// Backend-agnostic text-to-speech engine.
///
/// Implementations must be `Send + Sync` so the pipeline can hold them
/// across `.await` points behind a `tokio::sync::RwLock`.
///
/// The `synthesize` method is async (via [`async_trait`]) because some
/// backends (e.g., Kokoro) perform inference asynchronously.
#[async_trait::async_trait]
pub trait TtsBackend: Send + Sync {
    /// Synthesize text to audio.
    ///
    /// # Arguments
    /// * `text` — Text to synthesize (should be a single sentence or short
    ///   paragraph; the pipeline handles chunking).
    ///
    /// # Returns
    /// A [`TtsAudio`] containing the synthesized PCM samples, sample rate,
    /// and duration.
    async fn synthesize(&self, text: &str) -> Result<TtsAudio, VoiceError>;

    /// Change the active voice.
    fn set_voice(&mut self, voice_id: &str);

    /// Set playback speed multiplier (0.5–2.0).
    fn set_speed(&mut self, speed: f32);

    /// Get the current voice ID.
    fn voice(&self) -> &str;

    /// Get the output sample rate (Hz).
    fn sample_rate(&self) -> u32;

    /// List all available voices with metadata.
    fn available_voices(&self) -> Vec<VoiceInfo>;
}

// ── Backend-agnostic configuration ─────────────────────────────────

/// Backend-agnostic STT configuration.
///
/// Contains only the settings that are meaningful regardless of which STT
/// engine is active (Whisper, sherpa-onnx, etc.).  Backend constructors
/// can accept additional engine-specific options.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SttConfig {
    /// Language code (e.g., `"en"`). Empty string means auto-detect.
    pub language: String,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
        }
    }
}

/// Backend-agnostic TTS configuration.
///
/// Contains only the settings that are meaningful regardless of which TTS
/// engine is active (Kokoro, sherpa-onnx, etc.).  Backend constructors
/// can accept additional engine-specific options.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TtsConfig {
    /// Voice identifier (backend-specific meaning, e.g. `"af_sarah"`).
    pub voice: String,

    /// Playback speed multiplier (0.5–2.0, default 1.0).
    pub speed: f32,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            voice: "af_sarah".to_string(),
            speed: 1.0,
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────

/// Convenience constructor for [`VoiceInfo`].
pub(crate) fn voice_info(
    id: &str,
    name: &str,
    category: &str,
    gender: VoiceGender,
) -> VoiceInfo {
    VoiceInfo {
        id: id.to_string(),
        name: name.to_string(),
        category: category.to_string(),
        gender,
    }
}
