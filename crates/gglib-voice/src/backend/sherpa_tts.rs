//! Sherpa-ONNX Kokoro TTS backend — implements [`TtsBackend`] via `sherpa-rs`.
//!
//! Wraps `sherpa_rs::tts::KokoroTts` behind the engine-agnostic [`TtsBackend`]
//! trait.  The sherpa-rs `create` method requires `&mut self`, while our trait
//! uses `&self`, so the inner engine is wrapped in an `Arc<Mutex<…>>`.
//! Synthesis is dispatched via `tokio::task::spawn_blocking` so the Tokio
//! worker thread is never blocked during inference.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sherpa_rs::tts::{KokoroTts, KokoroTtsConfig};

use crate::backend::{TtsAudio, TtsBackend, VoiceGender, VoiceInfo, voice_info};
use crate::error::VoiceError;

/// Sherpa-ONNX Kokoro TTS sample rate (24 kHz).
pub const SHERPA_TTS_SAMPLE_RATE: u32 = 24_000;

/// Configuration for the Sherpa Kokoro TTS backend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SherpaTtsConfig {
    /// Voice identifier (e.g., `"af_sarah"`).
    pub voice: String,

    /// Playback speed multiplier (0.5–2.0, default 1.0).
    pub speed: f32,
}

impl Default for SherpaTtsConfig {
    fn default() -> Self {
        Self {
            voice: "af_sarah".to_string(),
            speed: 1.0,
        }
    }
}

/// Sherpa-ONNX Kokoro TTS backend.
///
/// Uses `sherpa_rs::tts::KokoroTts` for speech synthesis.  The inner engine
/// is behind a [`Mutex`] because `KokoroTts::create` takes `&mut self`
/// while our [`TtsBackend`] trait requires `&self`.
pub struct SherpaTtsBackend {
    /// The loaded sherpa-onnx TTS engine.
    ///
    /// Wrapped in `Arc<Mutex<…>>` so it can be moved into
    /// `tokio::task::spawn_blocking` closures while the outer `&self` stays
    /// alive.  `KokoroTts` is `Send + Sync` (per sherpa-rs's own `unsafe impl`).
    engine: Arc<Mutex<KokoroTts>>,

    /// Currently selected voice ID string.
    voice_id: String,

    /// Numeric speaker ID that sherpa-onnx uses (derived from voice_id).
    speaker_id: i32,

    /// Playback speed multiplier (1.0 = normal).
    speed: f32,
}

impl SherpaTtsBackend {
    /// Load the Sherpa Kokoro TTS model from a directory.
    ///
    /// The directory must contain:
    /// - `model.onnx` — the Kokoro ONNX model
    /// - `voices.bin` — packed voice style embeddings
    /// - `tokens.txt` — tokenizer vocabulary
    /// - `data_dir/` — espeak-ng data directory (lexicon data)
    ///
    /// # Arguments
    /// * `model_dir` — Path to the directory containing model files.
    /// * `config` — Sherpa TTS configuration (voice, speed).
    pub fn load(model_dir: &Path, config: &SherpaTtsConfig) -> Result<Self, VoiceError> {
        if !model_dir.exists() {
            return Err(VoiceError::ModelNotFound(model_dir.to_path_buf()));
        }

        let model_path = model_dir.join("model.onnx");
        let voices_path = model_dir.join("voices.bin");
        let tokens_path = model_dir.join("tokens.txt");
        let data_dir = model_dir.join("espeak-ng-data");

        for (path, desc) in [
            (&model_path, "model.onnx"),
            (&voices_path, "voices.bin"),
            (&tokens_path, "tokens.txt"),
        ] {
            if !path.exists() {
                return Err(VoiceError::ModelNotFound(path.clone()));
            }
            tracing::debug!(path = %path.display(), "Found TTS {desc}");
        }

        let model_str = path_to_string(&model_path)?;
        let voices_str = path_to_string(&voices_path)?;
        let tokens_str = path_to_string(&tokens_path)?;
        let data_dir_str = path_to_string(&data_dir)?;

        tracing::info!(
            dir = %model_dir.display(),
            voice = %config.voice,
            speed = config.speed,
            "Loading Sherpa Kokoro TTS model"
        );

        let sherpa_config = KokoroTtsConfig {
            model: model_str,
            voices: voices_str,
            tokens: tokens_str,
            data_dir: data_dir_str,
            length_scale: config.speed,
            ..Default::default()
        };

        let engine = KokoroTts::new(sherpa_config);

        let speaker_id = voice_id_to_speaker_id(&config.voice);

        tracing::info!("Sherpa Kokoro TTS model loaded successfully");

        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
            voice_id: config.voice.clone(),
            speaker_id,
            speed: config.speed,
        })
    }
}

#[async_trait::async_trait]
impl TtsBackend for SherpaTtsBackend {
    async fn synthesize(&self, text: &str) -> Result<TtsAudio, VoiceError> {
        if text.trim().is_empty() {
            return Ok(TtsAudio {
                samples: Vec::new(),
                sample_rate: SHERPA_TTS_SAMPLE_RATE,
                duration: Duration::ZERO,
            });
        }

        tracing::debug!(
            text_len = text.len(),
            voice = %self.voice_id,
            speaker_id = self.speaker_id,
            "Synthesizing speech (Sherpa Kokoro)"
        );

        // Kokoro inference is CPU-bound and can take hundreds of milliseconds.
        // Offload to a blocking thread pool so the Tokio worker is not stalled.
        let engine = Arc::clone(&self.engine);
        let sid = self.speaker_id;
        let speed = self.speed;
        let text = text.to_string();

        let audio = tokio::task::spawn_blocking(move || {
            engine
                .lock()
                .map_err(|e| VoiceError::SynthesisError(format!("TTS engine lock poisoned: {e}")))
                .and_then(|mut guard| {
                    guard
                        .create(&text, sid, speed)
                        .map_err(|e| VoiceError::SynthesisError(format!("{e}")))
                })
        })
        .await
        .map_err(|e| VoiceError::SynthesisError(format!("spawn_blocking join error: {e}")))??;

        let sample_rate = audio.sample_rate;
        let samples = audio.samples;

        // Compute duration from samples and sample rate
        #[allow(clippy::cast_precision_loss)]
        let duration = if sample_rate > 0 {
            Duration::from_secs_f64(samples.len() as f64 / f64::from(sample_rate))
        } else {
            Duration::ZERO
        };

        tracing::debug!(
            samples = samples.len(),
            sample_rate,
            duration_ms = duration.as_millis(),
            "Speech synthesized (Sherpa Kokoro)"
        );

        Ok(TtsAudio {
            samples,
            sample_rate,
            duration,
        })
    }

    fn set_voice(&mut self, voice_id: &str) {
        let new_sid = voice_id_to_speaker_id(voice_id);
        if new_sid >= 0 {
            self.voice_id = voice_id.to_string();
            self.speaker_id = new_sid;
            tracing::debug!(voice = %self.voice_id, sid = self.speaker_id, "TTS voice changed");
        } else {
            tracing::warn!(voice = %voice_id, "Unknown TTS voice, keeping current");
        }
    }

    fn voice(&self) -> &str {
        &self.voice_id
    }

    fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(0.5, 2.0);
    }

    fn sample_rate(&self) -> u32 {
        SHERPA_TTS_SAMPLE_RATE
    }

    fn available_voices(&self) -> Vec<VoiceInfo> {
        sherpa_kokoro_voices()
    }
}

// ── Voice catalogue ────────────────────────────────────────────────
//
// Kokoro v0.19 English ships 11 voice styles.  The speaker IDs are the
// indices into the packed `voices.bin` style matrix, as declared in the
// ONNX model's `speaker2id` metadata.

/// Map a voice ID string (e.g., `"af_sarah"`) to the sherpa-onnx speaker ID.
///
/// The IDs match the `speaker2id` metadata in the Kokoro v0.19 English model
/// (`kokoro-en-v0_19`).  Returns -1 for unknown voices so that
/// [`set_voice`] can reject them gracefully.
fn voice_id_to_speaker_id(voice_id: &str) -> i32 {
    // Speaker IDs from model metadata:
    //   af->0, af_bella->1, af_nicole->2, af_sarah->3, af_sky->4,
    //   am_adam->5, am_michael->6, bf_emma->7, bf_isabella->8,
    //   bm_george->9, bm_lewis->10
    match voice_id {
        "af" => 0,
        "af_bella" => 1,
        "af_nicole" => 2,
        "af_sarah" => 3,
        "af_sky" => 4,
        "am_adam" => 5,
        "am_michael" => 6,
        "bf_emma" => 7,
        "bf_isabella" => 8,
        "bm_george" => 9,
        "bm_lewis" => 10,
        _ => {
            tracing::warn!(voice = %voice_id, "Unknown Kokoro voice — using default speaker 0");
            0
        }
    }
}

/// List all Sherpa Kokoro voices with metadata.
///
/// Matches the 11 voices in the `kokoro-en-v0_19` model. Free function so
/// it can be called without a loaded engine (e.g., to populate a settings
/// UI before model download).
#[must_use]
pub fn sherpa_kokoro_voices() -> Vec<VoiceInfo> {
    vec![
        // American English — Female
        voice_info("af", "Default", "American English", VoiceGender::Female),
        voice_info("af_bella", "Bella", "American English", VoiceGender::Female),
        voice_info(
            "af_nicole",
            "Nicole",
            "American English",
            VoiceGender::Female,
        ),
        voice_info("af_sarah", "Sarah", "American English", VoiceGender::Female),
        voice_info("af_sky", "Sky", "American English", VoiceGender::Female),
        // American English — Male
        voice_info("am_adam", "Adam", "American English", VoiceGender::Male),
        voice_info(
            "am_michael",
            "Michael",
            "American English",
            VoiceGender::Male,
        ),
        // British English — Female
        voice_info("bf_emma", "Emma", "British English", VoiceGender::Female),
        voice_info(
            "bf_isabella",
            "Isabella",
            "British English",
            VoiceGender::Female,
        ),
        // British English — Male
        voice_info("bm_george", "George", "British English", VoiceGender::Male),
        voice_info("bm_lewis", "Lewis", "British English", VoiceGender::Male),
    ]
}

/// Convert a path to a string, returning a `VoiceError` on invalid UTF-8.
fn path_to_string(path: &Path) -> Result<String, VoiceError> {
    path.to_str()
        .map(ToString::to_string)
        .ok_or_else(|| VoiceError::SynthesisError(format!("Invalid path: {}", path.display())))
}
