//! Whisper STT backend — implements [`SttBackend`] for whisper.cpp via `whisper-rs`.
//!
//! Provides local speech transcription using whisper.cpp models in GGML format.

use std::path::Path;
use std::sync::Arc;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::backend::SttBackend;
use crate::error::VoiceError;

/// Configuration for the Whisper STT backend.
#[derive(Debug, Clone)]
pub struct WhisperConfig {
    /// Language code (e.g., "en", "auto" for multilingual detection).
    pub language: String,

    /// Whether to translate non-English speech to English.
    pub translate: bool,

    /// Number of threads for inference (0 = auto).
    pub n_threads: u32,

    /// Whether to suppress non-speech tokens.
    pub suppress_non_speech: bool,
}

impl Default for WhisperConfig {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            translate: false,
            n_threads: 0, // auto-detect
            suppress_non_speech: true,
        }
    }
}

/// Whisper STT backend.
///
/// Holds a loaded whisper.cpp model and provides transcription via
/// the [`SttBackend`] trait.
pub struct WhisperBackend {
    /// The loaded whisper context (thread-safe, shareable).
    context: Arc<WhisperContext>,

    /// Configuration for this backend instance.
    config: WhisperConfig,
}

impl WhisperBackend {
    /// Load a whisper model from disk.
    ///
    /// # Arguments
    /// * `model_path` — Path to a whisper GGML `.bin` model file.
    /// * `config` — Whisper-specific configuration.
    pub fn load(model_path: &Path, config: &WhisperConfig) -> Result<Self, VoiceError> {
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
            config: config.clone(),
        })
    }

    /// Build `FullParams` from the given config, pre-configured for
    /// short conversational utterances.
    fn build_params<'a>(config: &'a WhisperConfig) -> FullParams<'a, 'a> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        let lang = if config.language == "auto" {
            None
        } else {
            Some(config.language.as_str())
        };
        params.set_language(lang);
        params.set_translate(config.translate);

        if config.n_threads > 0 {
            #[allow(clippy::cast_possible_wrap)]
            params.set_n_threads(config.n_threads as i32);
        }

        // Optimize for short conversational utterances
        params.set_single_segment(true);
        params.set_no_timestamps(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_suppress_nst(config.suppress_non_speech);

        params
    }

    /// Collect all segments from a whisper state into a single string.
    fn collect_segments(state: &whisper_rs::WhisperState) -> String {
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

        text.trim().to_string()
    }
}

impl SttBackend for WhisperBackend {
    fn transcribe(&self, audio: &[f32]) -> Result<String, VoiceError> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let mut state = self
            .context
            .create_state()
            .map_err(|e| VoiceError::TranscriptionError(format!("Failed to create state: {e}")))?;

        let params = Self::build_params(&self.config);

        state
            .full(params, audio)
            .map_err(|e| VoiceError::TranscriptionError(format!("{e}")))?;

        let text = Self::collect_segments(&state);

        tracing::debug!(
            segments = state.full_n_segments(),
            chars = text.len(),
            "Transcription complete"
        );

        Ok(text)
    }

    fn transcribe_with_callback(
        &self,
        audio: &[f32],
        mut on_segment: Box<dyn FnMut(&str) + Send + 'static>,
    ) -> Result<String, VoiceError> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let mut state = self
            .context
            .create_state()
            .map_err(|e| VoiceError::TranscriptionError(format!("Failed to create state: {e}")))?;

        let mut params = Self::build_params(&self.config);

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

        Ok(Self::collect_segments(&state))
    }

    fn language(&self) -> &str {
        &self.config.language
    }
}
