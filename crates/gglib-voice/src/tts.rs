//! Text-to-Speech module — Kokoro v1.0 via `kokoro-tts`.
//!
//! Provides local speech synthesis using the Kokoro 82M model in ONNX format.
//! No espeak-ng or external dependencies — `kokoro-tts` includes a built-in
//! grapheme-to-phoneme pipeline.

use std::path::Path;
use std::time::Duration;

use kokoro_tts::Voice;

use crate::error::VoiceError;

/// Kokoro TTS sample rate (24 kHz).
pub const KOKORO_SAMPLE_RATE: u32 = 24_000;

/// TTS engine wrapper around `kokoro-tts`.
///
/// Holds a loaded Kokoro model and provides speech synthesis methods.
pub struct TtsEngine {
    /// The loaded Kokoro TTS instance.
    engine: kokoro_tts::KokoroTts,

    /// Currently selected voice enum variant.
    voice: Voice,

    /// Currently selected voice ID string (for serialization/display).
    voice_id: String,

    /// Playback speed multiplier (1.0 = normal).
    speed: f32,
}

/// Configuration for the TTS engine.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TtsConfig {
    /// Voice identifier (e.g., "af_sarah", "am_michael").
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

/// Information about an available TTS voice.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
pub enum VoiceGender {
    Female,
    Male,
}

impl TtsEngine {
    /// Load the Kokoro TTS model from disk.
    ///
    /// # Arguments
    /// * `model_path` — Path to the Kokoro ONNX model file
    /// * `voices_path` — Path to the voice styles data file
    /// * `config` — TTS configuration (voice selection, speed)
    pub async fn load(
        model_path: &Path,
        voices_path: &Path,
        config: &TtsConfig,
    ) -> Result<Self, VoiceError> {
        if !model_path.exists() {
            return Err(VoiceError::ModelNotFound(model_path.to_path_buf()));
        }
        if !voices_path.exists() {
            return Err(VoiceError::ModelNotFound(voices_path.to_path_buf()));
        }

        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| VoiceError::SynthesisError("Invalid model path".to_string()))?;
        let voices_path_str = voices_path
            .to_str()
            .ok_or_else(|| VoiceError::SynthesisError("Invalid voices path".to_string()))?;

        tracing::info!(
            model = %model_path.display(),
            voices = %voices_path.display(),
            voice = %config.voice,
            "Loading Kokoro TTS model"
        );

        let engine = kokoro_tts::KokoroTts::new(model_path_str, voices_path_str)
            .await
            .map_err(|e| VoiceError::SynthesisError(format!("Failed to load Kokoro model: {e}")))?;

        let voice = voice_from_id(&config.voice, config.speed)?;

        tracing::info!("Kokoro TTS model loaded successfully");

        Ok(Self {
            engine,
            voice,
            voice_id: config.voice.clone(),
            speed: config.speed,
        })
    }

    /// Synthesize text to audio samples.
    ///
    /// # Returns
    /// A tuple of (PCM f32 samples at 24 kHz, duration of the audio).
    pub async fn synthesize(&self, text: &str) -> Result<(Vec<f32>, Duration), VoiceError> {
        if text.trim().is_empty() {
            return Ok((Vec::new(), Duration::ZERO));
        }

        tracing::debug!(
            text_len = text.len(),
            voice = %self.voice_id,
            "Synthesizing speech"
        );

        let (samples, duration) = self
            .engine
            .synth(text, self.voice)
            .await
            .map_err(|e| VoiceError::SynthesisError(format!("{e}")))?;

        tracing::debug!(
            samples = samples.len(),
            duration_ms = duration.as_millis(),
            "Speech synthesized"
        );

        Ok((samples, duration))
    }

    /// Get a streaming synthesis handle.
    ///
    /// Returns a (sink, stream) pair. Send text sentences to the sink,
    /// and read audio chunks from the stream as they become available.
    pub fn synthesize_stream(
        &self,
    ) -> (kokoro_tts::SynthSink<String>, kokoro_tts::SynthStream) {
        self.engine.stream(self.voice)
    }

    /// Change the active voice.
    pub fn set_voice(&mut self, voice_id: &str) {
        if let Ok(v) = voice_from_id(voice_id, self.speed) {
            self.voice = v;
            self.voice_id = voice_id.to_string();
            tracing::debug!(voice = %self.voice_id, "TTS voice changed");
        } else {
            tracing::warn!(voice = %voice_id, "Unknown TTS voice, keeping current");
        }
    }

    /// Get the current voice ID.
    #[must_use]
    pub fn voice(&self) -> &str {
        &self.voice_id
    }

    /// Set playback speed (0.5–2.0).
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(0.5, 2.0);
    }

    /// Get the output sample rate (always 24 kHz for Kokoro).
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        KOKORO_SAMPLE_RATE
    }

    /// List all available voices with metadata.
    #[must_use]
    pub fn available_voices() -> Vec<VoiceInfo> {
        vec![
            // American English — Female
            voice("af_alloy", "Alloy", "American English", VoiceGender::Female),
            voice("af_aoede", "Aoede", "American English", VoiceGender::Female),
            voice("af_bella", "Bella", "American English", VoiceGender::Female),
            voice("af_heart", "Heart", "American English", VoiceGender::Female),
            voice("af_jessica", "Jessica", "American English", VoiceGender::Female),
            voice("af_nicole", "Nicole", "American English", VoiceGender::Female),
            voice("af_nova", "Nova", "American English", VoiceGender::Female),
            voice("af_river", "River", "American English", VoiceGender::Female),
            voice("af_sarah", "Sarah", "American English", VoiceGender::Female),
            voice("af_sky", "Sky", "American English", VoiceGender::Female),
            // American English — Male
            voice("am_adam", "Adam", "American English", VoiceGender::Male),
            voice("am_echo", "Echo", "American English", VoiceGender::Male),
            voice("am_eric", "Eric", "American English", VoiceGender::Male),
            voice("am_fable", "Fable", "American English", VoiceGender::Male),
            voice("am_liam", "Liam", "American English", VoiceGender::Male),
            voice("am_michael", "Michael", "American English", VoiceGender::Male),
            voice("am_onyx", "Onyx", "American English", VoiceGender::Male),
            voice("am_puck", "Puck", "American English", VoiceGender::Male),
            // British English — Female
            voice("bf_alice", "Alice", "British English", VoiceGender::Female),
            voice("bf_emma", "Emma", "British English", VoiceGender::Female),
            voice("bf_isabella", "Isabella", "British English", VoiceGender::Female),
            voice("bf_lily", "Lily", "British English", VoiceGender::Female),
            // British English — Male
            voice("bm_daniel", "Daniel", "British English", VoiceGender::Male),
            voice("bm_fable", "Fable (British)", "British English", VoiceGender::Male),
            voice("bm_george", "George", "British English", VoiceGender::Male),
            voice("bm_lewis", "Lewis", "British English", VoiceGender::Male),
        ]
    }
}

fn voice(id: &str, name: &str, category: &str, gender: VoiceGender) -> VoiceInfo {
    VoiceInfo {
        id: id.to_string(),
        name: name.to_string(),
        category: category.to_string(),
        gender,
    }
}

/// Convert a voice ID string (e.g., "af_sarah") to a `kokoro_tts::Voice` enum variant.
fn voice_from_id(id: &str, speed: f32) -> Result<Voice, VoiceError> {
    match id {
        "af_alloy" => Ok(Voice::AfAlloy(speed)),
        "af_aoede" => Ok(Voice::AfAoede(speed)),
        "af_bella" => Ok(Voice::AfBella(speed)),
        "af_heart" => Ok(Voice::AfHeart(speed)),
        "af_jessica" => Ok(Voice::AfJessica(speed)),
        "af_kore" => Ok(Voice::AfKore(speed)),
        "af_nicole" => Ok(Voice::AfNicole(speed)),
        "af_nova" => Ok(Voice::AfNova(speed)),
        "af_river" => Ok(Voice::AfRiver(speed)),
        "af_sarah" => Ok(Voice::AfSarah(speed)),
        "af_sky" => Ok(Voice::AfSky(speed)),
        "am_adam" => Ok(Voice::AmAdam(speed)),
        "am_echo" => Ok(Voice::AmEcho(speed)),
        "am_eric" => Ok(Voice::AmEric(speed)),
        "am_fable" => Ok(Voice::AmFenrir(speed)),
        "am_liam" => Ok(Voice::AmLiam(speed)),
        "am_michael" => Ok(Voice::AmMichael(speed)),
        "am_onyx" => Ok(Voice::AmOnyx(speed)),
        "am_puck" => Ok(Voice::AmPuck(speed)),
        "bf_alice" => Ok(Voice::BfAlice(speed)),
        "bf_emma" => Ok(Voice::BfEmma(speed)),
        "bf_isabella" => Ok(Voice::BfIsabella(speed)),
        "bf_lily" => Ok(Voice::BfLily(speed)),
        "bm_daniel" => Ok(Voice::BmDaniel(speed)),
        "bm_fable" => Ok(Voice::BmFable(speed)),
        "bm_george" => Ok(Voice::BmGeorge(speed)),
        "bm_lewis" => Ok(Voice::BmLewis(speed)),
        _ => Err(VoiceError::SynthesisError(format!(
            "Unknown voice: {id}"
        ))),
    }
}
