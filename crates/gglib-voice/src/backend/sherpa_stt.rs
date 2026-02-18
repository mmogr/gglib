//! Sherpa-ONNX Whisper STT backend — implements [`SttBackend`] via `sherpa-rs`.
//!
//! Wraps `sherpa_rs::whisper::WhisperRecognizer` behind the engine-agnostic
//! [`SttBackend`] trait.  The sherpa-rs `transcribe` method requires `&mut self`,
//! while our trait uses `&self`, so the inner recognizer is wrapped in an
//! `Arc<Mutex<…>>`.  Inference is dispatched via `tokio::task::spawn_blocking`
//! so the Tokio worker thread is never blocked during transcription.

use std::path::Path;
use std::sync::{Arc, Mutex};

use super::util;

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
    /// The loaded sherpa-onnx Whisper recognizer.
    ///
    /// Wrapped in `Arc<Mutex<…>>` so it can be moved into
    /// `tokio::task::spawn_blocking` closures while the outer `&self` stays
    /// alive.  `WhisperRecognizer` is `Send + Sync` (per sherpa-rs's own
    /// `unsafe impl`).
    recognizer: Arc<Mutex<WhisperRecognizer>>,

    /// Language code (stored for `language()` accessor).
    language: String,
}

impl SherpaSttBackend {
    /// Load a Sherpa Whisper model from a directory.
    ///
    /// The directory must contain encoder, decoder, and tokens files following
    /// the sherpa-onnx naming convention: `{prefix}-encoder.onnx`,
    /// `{prefix}-decoder.onnx`, `{prefix}-tokens.txt` (e.g.,
    /// `base.en-encoder.onnx`).  If an int8-quantised decoder is available
    /// (`{prefix}-decoder.int8.onnx`) it will be preferred over the
    /// full-precision variant.
    ///
    /// # Arguments
    /// * `model_dir` — Path to the directory containing model files.
    /// * `config` — Sherpa STT configuration.
    pub fn load(model_dir: &Path, config: &SherpaSttConfig) -> Result<Self, VoiceError> {
        if !model_dir.exists() {
            return Err(VoiceError::ModelNotFound(model_dir.to_path_buf()));
        }

        // Discover the file-name prefix used by sherpa-onnx archives.
        // Archives contain e.g. `base.en-encoder.onnx`, `base.en-decoder.onnx`,
        // `base.en-tokens.txt`.  We find the encoder and derive the prefix.
        let prefix = find_file_prefix(model_dir, "-encoder.onnx")?;

        let encoder_path = model_dir.join(format!("{prefix}-encoder.onnx"));

        // Prefer int8-quantised decoder if available (smaller + faster).
        let decoder_int8 = model_dir.join(format!("{prefix}-decoder.int8.onnx"));
        let decoder_path = if decoder_int8.exists() {
            decoder_int8
        } else {
            model_dir.join(format!("{prefix}-decoder.onnx"))
        };

        let tokens_path = model_dir.join(format!("{prefix}-tokens.txt"));

        for (path, desc) in [
            (&encoder_path, "encoder"),
            (&decoder_path, "decoder"),
            (&tokens_path, "tokens"),
        ] {
            if !path.exists() {
                return Err(VoiceError::ModelNotFound(path.clone()));
            }
            tracing::debug!(path = %path.display(), "Found STT {desc}");
        }

        let encoder_str = util::path_to_string(&encoder_path, VoiceError::ModelLoadError)?;
        let decoder_str = util::path_to_string(&decoder_path, VoiceError::ModelLoadError)?;
        let tokens_str = util::path_to_string(&tokens_path, VoiceError::ModelLoadError)?;

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
            recognizer: Arc::new(Mutex::new(recognizer)),
            language: display_language,
        })
    }
}

#[async_trait::async_trait]
impl SttBackend for SherpaSttBackend {
    async fn transcribe(&self, audio: &[f32]) -> Result<String, VoiceError> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        #[allow(clippy::cast_precision_loss)]
        let duration_secs = audio.len() as f64 / f64::from(SHERPA_STT_SAMPLE_RATE);
        tracing::debug!(
            samples = audio.len(),
            duration_secs,
            "Transcribing audio (Sherpa Whisper)"
        );

        // Whisper inference is CPU-bound and can take hundreds of milliseconds.
        // Offload to a blocking thread pool so the Tokio worker is not stalled.
        let recognizer = Arc::clone(&self.recognizer);
        let audio = audio.to_vec();

        let text = tokio::task::spawn_blocking(move || {
            let result = recognizer
                .lock()
                .map_err(|e| {
                    VoiceError::TranscriptionError(format!("STT recognizer lock poisoned: {e}"))
                })?
                .transcribe(SHERPA_STT_SAMPLE_RATE, &audio);
            Ok::<String, VoiceError>(result.text.trim().to_string())
        })
        .await
        .map_err(|e| VoiceError::TranscriptionError(format!("spawn_blocking join error: {e}")))??;

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
        // callbacks.  This sync method locks and runs inference directly —
        // it is safe to call from non-async contexts.  In async contexts,
        // prefer transcribe() which offloads to spawn_blocking.
        if audio.is_empty() {
            return Ok(String::new());
        }
        let text = self
            .recognizer
            .lock()
            .map_err(|e| {
                VoiceError::TranscriptionError(format!("STT recognizer lock poisoned: {e}"))
            })?
            .transcribe(SHERPA_STT_SAMPLE_RATE, audio)
            .text
            .trim()
            .to_string();
        if !text.is_empty() {
            on_segment(&text);
        }
        Ok(text)
    }

    fn language(&self) -> &str {
        &self.language
    }
}

/// Scan `dir` for a file whose name ends with `suffix` and return the prefix.
///
/// For example, given suffix `"-encoder.onnx"` and a file named
/// `"base.en-encoder.onnx"`, this returns `"base.en"`.
fn find_file_prefix(dir: &Path, suffix: &str) -> Result<String, VoiceError> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        VoiceError::ModelLoadError(format!(
            "Cannot read model directory {}: {e}",
            dir.display()
        ))
    })?;

    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if let Some(stripped) = name.strip_suffix(suffix) {
                return Ok(stripped.to_string());
            }
        }
    }

    Err(VoiceError::ModelLoadError(format!(
        "No file matching *{suffix} found in {}",
        dir.display()
    )))
}
