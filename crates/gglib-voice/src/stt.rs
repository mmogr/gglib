//! Speech-to-Text module — whisper.cpp via `whisper-rs`.
//!
//! Provides local speech transcription using whisper.cpp models in GGML format.
//! Models are loaded lazily on first use and kept resident while voice mode
//! is active.

use std::path::Path;
use std::sync::Arc;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::error::VoiceError;

/// Whisper STT engine wrapper.
///
/// Holds a loaded whisper model and provides transcription methods.
pub struct SttEngine {
    /// The loaded whisper context (thread-safe, shareable).
    context: Arc<WhisperContext>,

    /// Language to use for transcription (e.g., "en").
    language: String,
}

/// Configuration for the STT engine.
#[derive(Debug, Clone)]
pub struct SttConfig {
    /// Language code (e.g., "en", "auto" for multilingual detection).
    pub language: String,

    /// Whether to translate non-English speech to English.
    pub translate: bool,

    /// Number of threads for inference (0 = auto).
    pub n_threads: u32,

    /// Whether to suppress non-speech tokens.
    pub suppress_non_speech: bool,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            translate: false,
            n_threads: 0, // auto-detect
            suppress_non_speech: true,
        }
    }
}

impl SttEngine {
    /// Load a whisper model from disk.
    ///
    /// # Arguments
    /// * `model_path` — Path to a whisper GGML `.bin` model file
    /// * `config` — STT configuration
    pub fn load(model_path: &Path, config: &SttConfig) -> Result<Self, VoiceError> {
        if !model_path.exists() {
            return Err(VoiceError::ModelNotFound(model_path.to_path_buf()));
        }

        let model_path_str = model_path
            .to_str()
            .ok_or_else(|| VoiceError::WhisperLoadError("Invalid model path".to_string()))?;

        tracing::info!(path = %model_path.display(), "Loading whisper model");

        let params = WhisperContextParameters::default();
        let context = WhisperContext::new_with_params(model_path_str, params)
            .map_err(|e| VoiceError::WhisperLoadError(format!("{e}")))?;

        tracing::info!("Whisper model loaded successfully");

        Ok(Self {
            context: Arc::new(context),
            language: config.language.clone(),
        })
    }

    /// Transcribe audio samples to text.
    ///
    /// # Arguments
    /// * `audio` — PCM f32 samples at 16 kHz mono (use [`capture::AudioCapture`]
    ///   which handles resampling)
    ///
    /// # Returns
    /// The transcribed text, or an empty string if no speech was detected.
    pub fn transcribe(&self, audio: &[f32]) -> Result<String, VoiceError> {
        self.transcribe_with_config(audio, &SttConfig::default())
    }

    /// Transcribe audio with custom configuration.
    pub fn transcribe_with_config(
        &self,
        audio: &[f32],
        config: &SttConfig,
    ) -> Result<String, VoiceError> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let mut state = self
            .context
            .create_state()
            .map_err(|e| VoiceError::TranscriptionError(format!("Failed to create state: {e}")))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        // Language setting
        let lang = if config.language == "auto" {
            None
        } else {
            Some(config.language.as_str())
        };
        params.set_language(lang);

        // Translation mode
        params.set_translate(config.translate);

        // Threading
        if config.n_threads > 0 {
            params.set_n_threads(config.n_threads as i32);
        }

        // Optimize for short conversational utterances
        params.set_single_segment(true);
        params.set_no_timestamps(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_suppress_nst(config.suppress_non_speech);

        // Run inference
        state
            .full(params, audio)
            .map_err(|e| VoiceError::TranscriptionError(format!("{e}")))?;

        // Collect all segments into a single string
        let num_segments = state.full_n_segments();

        let mut text = String::new();
        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                if let Ok(segment_text) = segment.to_str() {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(segment_text.trim());
                }
            }
        }

        let text = text.trim().to_string();

        tracing::debug!(
            segments = num_segments,
            chars = text.len(),
            "Transcription complete"
        );

        Ok(text)
    }

    /// Transcribe audio with a segment callback for streaming partial results.
    ///
    /// The callback is invoked for each transcribed segment as it becomes available.
    pub fn transcribe_with_callback<F>(
        &self,
        audio: &[f32],
        mut on_segment: F,
    ) -> Result<String, VoiceError>
    where
        F: FnMut(&str) + Send + 'static,
    {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let mut state = self
            .context
            .create_state()
            .map_err(|e| VoiceError::TranscriptionError(format!("Failed to create state: {e}")))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        let lang = if self.language == "auto" {
            None
        } else {
            Some(self.language.as_str())
        };
        params.set_language(lang);
        params.set_single_segment(true);
        params.set_no_timestamps(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_suppress_nst(true);

        // Set segment callback for streaming results
        params.set_segment_callback_safe(move |data: whisper_rs::SegmentCallbackData| {
            let text = data.text.trim();
            if !text.is_empty() {
                on_segment(text);
            }
        });

        state
            .full(params, audio)
            .map_err(|e| VoiceError::TranscriptionError(format!("{e}")))?;

        // Collect all segments
        let num_segments = state.full_n_segments();

        let mut text = String::new();
        for i in 0..num_segments {
            if let Some(segment) = state.get_segment(i) {
                if let Ok(segment_text) = segment.to_str() {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(segment_text.trim());
                }
            }
        }

        Ok(text.trim().to_string())
    }

    /// Get the model's language.
    #[must_use]
    pub fn language(&self) -> &str {
        &self.language
    }
}
