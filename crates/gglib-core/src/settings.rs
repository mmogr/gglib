//! Settings domain types and validation.
//!
//! This module contains the core settings types used across the application.
//! These are pure domain types with no infrastructure dependencies.

use serde::{Deserialize, Serialize};

use crate::domain::InferenceConfig;

/// Default port for the OpenAI-compatible proxy server.
pub const DEFAULT_PROXY_PORT: u16 = 8080;

/// Default base port for llama-server instance allocation.
pub const DEFAULT_LLAMA_BASE_PORT: u16 = 9000;

/// Application settings structure.
///
/// All fields are optional to support partial updates and graceful defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Default directory for downloading models.
    pub default_download_path: Option<String>,

    /// Default context size for models (e.g., 4096, 8192).
    pub default_context_size: Option<u64>,

    /// Port for the OpenAI-compatible proxy server.
    pub proxy_port: Option<u16>,

    /// Base port for llama-server instance allocation (first port in range).
    /// Note: The OpenAI-compatible proxy listens on `proxy_port`.
    pub llama_base_port: Option<u16>,

    /// Maximum number of downloads that can be queued (1-50).
    pub max_download_queue_size: Option<u32>,

    /// Whether to show memory fit indicators in `HuggingFace` browser.
    pub show_memory_fit_indicators: Option<bool>,

    /// Maximum iterations for tool calling agentic loop.
    pub max_tool_iterations: Option<u32>,

    /// Maximum stagnation steps before stopping agent loop.
    pub max_stagnation_steps: Option<u32>,

    /// Default model ID for commands that support a default model.
    pub default_model_id: Option<i64>,

    /// Global inference parameter defaults.
    ///
    /// Applied when neither request nor per-model defaults are specified.
    /// If not set, hardcoded defaults are used as final fallback.
    #[serde(default)]
    pub inference_defaults: Option<InferenceConfig>,

    // ── Voice settings ─────────────────────────────────────────────
    /// Whether voice mode is enabled.
    pub voice_enabled: Option<bool>,

    /// Voice interaction mode: "ptt" (push-to-talk) or "vad" (voice activity detection).
    pub voice_interaction_mode: Option<String>,

    /// Selected whisper STT model ID (e.g., "base.en", "small.en-q5_1").
    pub voice_stt_model: Option<String>,

    /// Selected TTS voice ID (e.g., `af_sarah`, `am_michael`).
    pub voice_tts_voice: Option<String>,

    /// TTS playback speed multiplier (0.5–2.0, default 1.0).
    pub voice_tts_speed: Option<f32>,

    /// VAD speech detection threshold (0.0–1.0, default 0.5).
    pub voice_vad_threshold: Option<f32>,

    /// VAD minimum silence duration in ms before utterance ends (default 700).
    pub voice_vad_silence_ms: Option<u32>,

    /// Whether to automatically speak LLM responses via TTS.
    pub voice_auto_speak: Option<bool>,

    /// Preferred audio input device name (None = system default).
    pub voice_input_device: Option<String>,
}

impl Settings {
    /// Create settings with sensible defaults.
    #[must_use]
    pub const fn with_defaults() -> Self {
        Self {
            default_download_path: None,
            default_context_size: Some(4096),
            proxy_port: Some(DEFAULT_PROXY_PORT),
            llama_base_port: Some(DEFAULT_LLAMA_BASE_PORT),
            max_download_queue_size: Some(10),
            show_memory_fit_indicators: Some(true),
            max_tool_iterations: Some(25),
            max_stagnation_steps: Some(5),
            default_model_id: None,
            inference_defaults: None,
            voice_enabled: Some(false),
            voice_interaction_mode: None,
            voice_stt_model: None,
            voice_tts_voice: None,
            voice_tts_speed: Some(1.0),
            voice_vad_threshold: None,
            voice_vad_silence_ms: None,
            voice_auto_speak: Some(true),
            voice_input_device: None,
        }
    }

    /// Get the effective proxy port (with default fallback).
    #[must_use]
    pub const fn effective_proxy_port(&self) -> u16 {
        match self.proxy_port {
            Some(port) => port,
            None => DEFAULT_PROXY_PORT,
        }
    }

    /// Get the effective llama-server base port (with default fallback).
    #[must_use]
    pub const fn effective_llama_base_port(&self) -> u16 {
        match self.llama_base_port {
            Some(port) => port,
            None => DEFAULT_LLAMA_BASE_PORT,
        }
    }

    /// Merge another settings into this one, only updating fields that are Some.
    pub fn merge(&mut self, other: &SettingsUpdate) {
        if let Some(ref path) = other.default_download_path {
            self.default_download_path.clone_from(path);
        }
        if let Some(ref ctx_size) = other.default_context_size {
            self.default_context_size = *ctx_size;
        }
        if let Some(ref port) = other.proxy_port {
            self.proxy_port = *port;
        }
        if let Some(ref port) = other.llama_base_port {
            self.llama_base_port = *port;
        }
        if let Some(ref queue_size) = other.max_download_queue_size {
            self.max_download_queue_size = *queue_size;
        }
        if let Some(ref show_fit) = other.show_memory_fit_indicators {
            self.show_memory_fit_indicators = *show_fit;
        }
        if let Some(ref iters) = other.max_tool_iterations {
            self.max_tool_iterations = *iters;
        }
        if let Some(ref steps) = other.max_stagnation_steps {
            self.max_stagnation_steps = *steps;
        }
        if let Some(ref model_id) = other.default_model_id {
            self.default_model_id = *model_id;
        }
        if let Some(ref inference_defaults) = other.inference_defaults {
            self.inference_defaults.clone_from(inference_defaults);
        }
        if let Some(ref v) = other.voice_enabled {
            self.voice_enabled = *v;
        }
        if let Some(ref v) = other.voice_interaction_mode {
            self.voice_interaction_mode.clone_from(v);
        }
        if let Some(ref v) = other.voice_stt_model {
            self.voice_stt_model.clone_from(v);
        }
        if let Some(ref v) = other.voice_tts_voice {
            self.voice_tts_voice.clone_from(v);
        }
        if let Some(ref v) = other.voice_tts_speed {
            self.voice_tts_speed = *v;
        }
        if let Some(ref v) = other.voice_vad_threshold {
            self.voice_vad_threshold = *v;
        }
        if let Some(ref v) = other.voice_vad_silence_ms {
            self.voice_vad_silence_ms = *v;
        }
        if let Some(ref v) = other.voice_auto_speak {
            self.voice_auto_speak = *v;
        }
        if let Some(ref v) = other.voice_input_device {
            self.voice_input_device.clone_from(v);
        }
    }
}

/// Partial settings update.
///
/// Each field is `Option<Option<T>>`:
/// - `None` = don't change this field
/// - `Some(None)` = set field to None/null
/// - `Some(Some(value))` = set field to value
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SettingsUpdate {
    pub default_download_path: Option<Option<String>>,
    pub default_context_size: Option<Option<u64>>,
    pub proxy_port: Option<Option<u16>>,
    pub llama_base_port: Option<Option<u16>>,
    pub max_download_queue_size: Option<Option<u32>>,
    pub show_memory_fit_indicators: Option<Option<bool>>,
    pub max_tool_iterations: Option<Option<u32>>,
    pub max_stagnation_steps: Option<Option<u32>>,
    pub default_model_id: Option<Option<i64>>,
    pub inference_defaults: Option<Option<InferenceConfig>>,
    pub voice_enabled: Option<Option<bool>>,
    pub voice_interaction_mode: Option<Option<String>>,
    pub voice_stt_model: Option<Option<String>>,
    pub voice_tts_voice: Option<Option<String>>,
    pub voice_tts_speed: Option<Option<f32>>,
    pub voice_vad_threshold: Option<Option<f32>>,
    pub voice_vad_silence_ms: Option<Option<u32>>,
    pub voice_auto_speak: Option<Option<bool>>,
    pub voice_input_device: Option<Option<String>>,
}

/// Settings validation error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SettingsError {
    #[error("Context size must be between 512 and 1,000,000, got {0}")]
    InvalidContextSize(u64),

    #[error("Port should be >= 1024 (privileged ports require root), got {0}")]
    InvalidPort(u16),

    #[error("Max download queue size must be between 1 and 50, got {0}")]
    InvalidQueueSize(u32),

    #[error("Download path cannot be empty")]
    EmptyDownloadPath,

    #[error("Invalid inference parameter: {0}")]
    InvalidInferenceConfig(String),
}

/// Validate settings values.
pub fn validate_settings(settings: &Settings) -> Result<(), SettingsError> {
    // Validate context size
    if let Some(ctx_size) = settings.default_context_size {
        if !(512..=1_000_000).contains(&ctx_size) {
            return Err(SettingsError::InvalidContextSize(ctx_size));
        }
    }

    // Validate proxy port
    if let Some(port) = settings.proxy_port {
        if port < 1024 {
            return Err(SettingsError::InvalidPort(port));
        }
    }

    // Validate llama-server base port
    if let Some(port) = settings.llama_base_port {
        if port < 1024 {
            return Err(SettingsError::InvalidPort(port));
        }
    }

    // Validate max download queue size
    if let Some(queue_size) = settings.max_download_queue_size {
        if !(1..=50).contains(&queue_size) {
            return Err(SettingsError::InvalidQueueSize(queue_size));
        }
    }

    // Validate download path if specified
    if settings
        .default_download_path
        .as_ref()
        .is_some_and(|p| p.trim().is_empty())
    {
        return Err(SettingsError::EmptyDownloadPath);
    }

    // Validate inference defaults if specified
    if let Some(ref inference_config) = settings.inference_defaults {
        validate_inference_config(inference_config)
            .map_err(SettingsError::InvalidInferenceConfig)?;
    }

    Ok(())
}

/// Validate inference configuration parameters.
///
/// Checks that all specified parameters are within valid ranges.
pub fn validate_inference_config(config: &InferenceConfig) -> Result<(), String> {
    // Validate temperature (0.0 - 2.0)
    if let Some(temp) = config.temperature {
        if !(0.0..=2.0).contains(&temp) {
            return Err(format!(
                "Temperature must be between 0.0 and 2.0, got {temp}"
            ));
        }
    }

    // Validate top_p (0.0 - 1.0)
    if let Some(top_p) = config.top_p {
        if !(0.0..=1.0).contains(&top_p) {
            return Err(format!("Top P must be between 0.0 and 1.0, got {top_p}"));
        }
    }

    // Validate top_k (must be positive)
    if let Some(top_k) = config.top_k {
        if top_k <= 0 {
            return Err(format!("Top K must be positive, got {top_k}"));
        }
    }

    // Validate max_tokens (must be positive)
    if let Some(max_tokens) = config.max_tokens {
        if max_tokens == 0 {
            return Err("Max tokens must be positive".to_string());
        }
    }

    // Validate repeat_penalty (must be positive)
    if let Some(repeat_penalty) = config.repeat_penalty {
        if repeat_penalty <= 0.0 {
            return Err(format!(
                "Repeat penalty must be positive, got {repeat_penalty}"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = Settings::with_defaults();
        assert_eq!(settings.default_context_size, Some(4096));
        assert_eq!(settings.proxy_port, Some(DEFAULT_PROXY_PORT));
        assert_eq!(settings.llama_base_port, Some(DEFAULT_LLAMA_BASE_PORT));
        assert_eq!(settings.default_download_path, None);
        assert_eq!(settings.max_download_queue_size, Some(10));
        assert_eq!(settings.show_memory_fit_indicators, Some(true));
    }

    #[test]
    fn test_validate_settings_valid() {
        let settings = Settings::with_defaults();
        assert!(validate_settings(&settings).is_ok());
    }

    #[test]
    fn test_validate_context_size_too_small() {
        let settings = Settings {
            default_context_size: Some(100),
            ..Default::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::InvalidContextSize(100))
        ));
    }

    #[test]
    fn test_validate_context_size_too_large() {
        let settings = Settings {
            default_context_size: Some(2_000_000),
            ..Default::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::InvalidContextSize(2_000_000))
        ));
    }

    #[test]
    fn test_validate_port_too_low() {
        let settings = Settings {
            proxy_port: Some(80),
            ..Default::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::InvalidPort(80))
        ));
    }

    #[test]
    fn test_validate_empty_path() {
        let settings = Settings {
            default_download_path: Some(String::new()),
            ..Default::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::EmptyDownloadPath)
        ));
    }

    #[test]
    fn test_validate_inference_config_valid() {
        let config = InferenceConfig {
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
            max_tokens: Some(2048),
            repeat_penalty: Some(1.1),
        };
        assert!(validate_inference_config(&config).is_ok());
    }

    #[test]
    fn test_validate_inference_config_temperature_out_of_range() {
        let config = InferenceConfig {
            temperature: Some(2.5),
            ..Default::default()
        };
        assert!(validate_inference_config(&config).is_err());

        let config = InferenceConfig {
            temperature: Some(-0.1),
            ..Default::default()
        };
        assert!(validate_inference_config(&config).is_err());
    }

    #[test]
    fn test_validate_inference_config_top_p_out_of_range() {
        let config = InferenceConfig {
            top_p: Some(1.5),
            ..Default::default()
        };
        assert!(validate_inference_config(&config).is_err());

        let config = InferenceConfig {
            top_p: Some(-0.1),
            ..Default::default()
        };
        assert!(validate_inference_config(&config).is_err());
    }

    #[test]
    fn test_validate_inference_config_negative_values() {
        let config = InferenceConfig {
            top_k: Some(-1),
            ..Default::default()
        };
        assert!(validate_inference_config(&config).is_err());

        let config = InferenceConfig {
            repeat_penalty: Some(0.0),
            ..Default::default()
        };
        assert!(validate_inference_config(&config).is_err());
    }

    #[test]
    fn test_settings_with_valid_inference_defaults() {
        let settings = Settings {
            inference_defaults: Some(InferenceConfig {
                temperature: Some(0.8),
                top_p: Some(0.95),
                ..Default::default()
            }),
            ..Settings::with_defaults()
        };
        assert!(validate_settings(&settings).is_ok());
    }

    #[test]
    fn test_settings_with_invalid_inference_defaults() {
        let settings = Settings {
            inference_defaults: Some(InferenceConfig {
                temperature: Some(3.0), // Invalid
                ..Default::default()
            }),
            ..Settings::with_defaults()
        };
        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn test_validate_queue_size_too_small() {
        let settings = Settings {
            max_download_queue_size: Some(0),
            ..Default::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::InvalidQueueSize(0))
        ));
    }

    #[test]
    fn test_validate_queue_size_too_large() {
        let settings = Settings {
            max_download_queue_size: Some(100),
            ..Default::default()
        };
        assert!(matches!(
            validate_settings(&settings),
            Err(SettingsError::InvalidQueueSize(100))
        ));
    }

    #[test]
    fn test_merge_settings() {
        let mut settings = Settings::with_defaults();
        let update = SettingsUpdate {
            default_context_size: Some(Some(8192)),
            proxy_port: Some(None), // Clear proxy port
            ..Default::default()
        };
        settings.merge(&update);

        assert_eq!(settings.default_context_size, Some(8192));
        assert_eq!(settings.proxy_port, None);
        assert_eq!(settings.llama_base_port, Some(DEFAULT_LLAMA_BASE_PORT)); // Unchanged
    }

    #[test]
    fn test_effective_ports() {
        let settings = Settings::with_defaults();
        assert_eq!(settings.effective_proxy_port(), DEFAULT_PROXY_PORT);
        assert_eq!(
            settings.effective_llama_base_port(),
            DEFAULT_LLAMA_BASE_PORT
        );

        let settings_none = Settings::default();
        assert_eq!(settings_none.effective_proxy_port(), DEFAULT_PROXY_PORT);
        assert_eq!(
            settings_none.effective_llama_base_port(),
            DEFAULT_LLAMA_BASE_PORT
        );
    }
}
