//! Sherpa-ONNX Whisper STT backend — implements [`SttBackend`] via `sherpa-rs`.
//!
//! Wraps `sherpa_rs::whisper::WhisperRecognizer` behind the engine-agnostic
//! [`SttBackend`] trait.  The sherpa-rs `transcribe` method requires `&mut self`,
//! while our trait uses `&self`, so the inner recognizer is wrapped in a
//! [`std::sync::Mutex`].

use std::path::Path;
use std::sync::Mutex;

use sherpa_rs::whisper::{WhisperConfig, WhisperRecognizer};

use crate::backend::SttBackend;
use crate::error::VoiceError;

/// Sherpa-ONNX expects 16 kHz mono audio.
pub const SHERPA_STT_SAMPLE_RATE: u32 = 16_000;

/// Configuration for the Sherpa Whisper STT backend.
#[derive(Debug, Clone)]
pub struct SherpaSttConfig {
    /// Language code (e.g., `"en"`, `"auto"` for multilingual detection).
    pub language: String,

    /// Number of threads for inference (0 = auto).
    pub num_threads: i32,
}

impl Default for SherpaSttConfig {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            num_threads: 4,
        }
    }
}

/// Sherpa-ONNX Whisper STT backend.
///
/// Uses `sherpa_rs::whisper::WhisperRecognizer` for speech transcription.
/// The inner recognizer is behind a [`Mutex`] because
/// `WhisperRecognizer::transcribe` takes `&mut self` while our
/// [`SttBackend`] trait requires `&self`.
pub struct SherpaSttBackend {
    /// The loaded sherpa-onnx Whisper recognizer (behind Mutex for interior mutability).
    recognizer: Mutex<WhisperRecognizer>,

    /// Language code (stored for `language()` accessor).
    language: String,
}

impl SherpaSttBackend {
    /// Load a Sherpa Whisper model from a directory.
    ///
    /// The directory must contain:
    /// - `encoder.onnx` — Whisper encoder model
    /// - `decoder.onnx` — Whisper decoder model
    /// - `tokens.txt` — tokenizer vocabulary
    ///
    /// # Arguments
    /// * `model_dir` — Path to the directory containing model files.
    /// * `config` — Sherpa STT configuration.
    pub fn load(model_dir: &Path, config: &SherpaSttConfig) -> Result<Self, VoiceError> {
        if !model_dir.exists() {
            return Err(VoiceError::ModelNotFound(model_dir.to_path_buf()));
        }

        let encoder_path = model_dir.join("encoder.onnx");
        let decoder_path = model_dir.join("decoder.onnx");
        let tokens_path = model_dir.join("tokens.txt");

        for (path, desc) in [
            (&encoder_path, "encoder.onnx"),
            (&decoder_path, "decoder.onnx"),
            (&tokens_path, "tokens.txt"),
        ] {
            if !path.exists() {
                return Err(VoiceError::ModelNotFound(path.clone()));
            }
            tracing::debug!(path = %path.display(), "Found STT {desc}");
        }

        let encoder_str = path_to_string(&encoder_path)?;
        let decoder_str = path_to_string(&decoder_path)?;
        let tokens_str = path_to_string(&tokens_path)?;

        tracing::info!(
            dir = %model_dir.display(),
            language = %config.language,
            num_threads = config.num_threads,
            "Loading Sherpa Whisper STT model"
        );

        let language = if config.language == "auto" {
            String::new()
        } else {
            config.language.clone()
        };

        let whisper_config = WhisperConfig {
            encoder: encoder_str,
            decoder: decoder_str,
            tokens: tokens_str,
            language: language.clone(),
            num_threads: Some(config.num_threads),
            ..Default::default()
        };

        let recognizer = WhisperRecognizer::new(whisper_config).map_err(|e| {
            VoiceError::ModelLoadError(format!("Failed to load Sherpa Whisper model: {e}"))
        })?;

        tracing::info!("Sherpa Whisper STT model loaded successfully");

        let display_language = if language.is_empty() {
            "auto".to_string()
        } else {
            language
        };

        Ok(Self {
            recognizer: Mutex::new(recognizer),
            language: display_language,
        })
    }
}

impl SttBackend for SherpaSttBackend {
    fn transcribe(&self, audio: &[f32]) -> Result<String, VoiceError> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        tracing::debug!(
            samples = audio.len(),
            duration_secs = audio.len() as f64 / f64::from(SHERPA_STT_SAMPLE_RATE),
            "Transcribing audio (Sherpa Whisper)"
        );

        let mut recognizer = self.recognizer.lock().map_err(|e| {
            VoiceError::TranscriptionError(format!("STT recognizer lock poisoned: {e}"))
        })?;

        let result = recognizer.transcribe(SHERPA_STT_SAMPLE_RATE, audio);
        let text = result.text.trim().to_string();

        tracing::debug!(
            chars = text.len(),
            "Transcription complete (Sherpa Whisper)"
        );

        Ok(text)
    }

    fn transcribe_with_callback(
        &self,
        audio: &[f32],
        mut on_segment: Box<dyn FnMut(&str) + Send + 'static>,
    ) -> Result<String, VoiceError> {
        // sherpa-rs WhisperRecognizer does not support streaming segment
        // callbacks, so we fall back to full transcription and invoke the
        // callback once with the complete result.
        let text = self.transcribe(audio)?;
        if !text.is_empty() {
            on_segment(&text);
        }
        Ok(text)
    }

    fn language(&self) -> &str {
        &self.language
    }
}

/// Convert a path to a string, returning a `VoiceError` on invalid UTF-8.
fn path_to_string(path: &Path) -> Result<String, VoiceError> {
    path.to_str()
        .map(ToString::to_string)
        .ok_or_else(|| {
            VoiceError::ModelLoadError(format!("Invalid path: {}", path.display()))
        })
}
