//! Inference configuration types.
//!
//! Defines shared types for configuring LLM inference parameters
//! (temperature, top_p, top_k, max_tokens, repeat_penalty).
//!
//! This module provides the core `InferenceConfig` type that is reused across:
//! - Per-model defaults (Model.inference_defaults)
//! - Global settings (Settings.inference_defaults)
//! - Request-level overrides (flattened in ChatProxyRequest)

use serde::{Deserialize, Serialize};

/// Inference parameters for LLM sampling.
///
/// All fields are optional to support partial configuration and fallback chains.
/// Intended to be shared across model defaults, global settings, and request overrides.
///
/// # Hierarchy Resolution
///
/// When making an inference request, parameters are resolved in this order:
/// 1. Request-level override (user specified for this request)
/// 2. Per-model defaults (stored in Model.inference_defaults)
/// 3. Global settings (stored in Settings.inference_defaults)
/// 4. Hardcoded fallback (e.g., temperature = 0.7)
///
/// # Examples
///
/// ```rust
/// use gglib_core::domain::InferenceConfig;
///
/// // Conservative settings for code generation
/// let code_gen = InferenceConfig {
///     temperature: Some(0.2),
///     top_p: Some(0.9),
///     top_k: Some(40),
///     max_tokens: Some(2048),
///     repeat_penalty: Some(1.1),
/// };
///
/// // Creative writing settings
/// let creative = InferenceConfig {
///     temperature: Some(1.2),
///     top_p: Some(0.95),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InferenceConfig {
    /// Sampling temperature (0.0 - 2.0).
    ///
    /// Controls randomness in token selection:
    /// - Lower values (0.1-0.5): More deterministic, focused
    /// - Medium values (0.7-1.0): Balanced creativity
    /// - Higher values (1.1-2.0): More random, creative
    pub temperature: Option<f32>,

    /// Nucleus sampling threshold (0.0 - 1.0).
    ///
    /// Considers only the top tokens whose cumulative probability exceeds this threshold.
    /// Common values: 0.9 (default), 0.95 (more diverse)
    pub top_p: Option<f32>,

    /// Top-K sampling limit.
    ///
    /// Considers only the K most likely next tokens.
    /// Common values: 40 (default), 10 (focused), 100 (diverse)
    pub top_k: Option<i32>,

    /// Maximum tokens to generate in response.
    ///
    /// Hard limit on response length. Does not include input tokens.
    pub max_tokens: Option<u32>,

    /// Repetition penalty (> 0.0, typically 1.0 - 1.3).
    ///
    /// Penalizes repeated tokens to reduce repetitive output.
    /// - 1.0: No penalty (default)
    /// - 1.1-1.3: Moderate penalty
    /// - > 1.3: Strong penalty (may hurt coherence)
    pub repeat_penalty: Option<f32>,
}

impl InferenceConfig {
    /// Merge another config into this one, preferring values from `other`.
    ///
    /// For each field, if `other` has Some(value), use it; otherwise keep self's value.
    /// This is useful for applying fallback chains.
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::InferenceConfig;
    ///
    /// let mut request = InferenceConfig {
    ///     temperature: Some(0.8),
    ///     ..Default::default()
    /// };
    ///
    /// let model_defaults = InferenceConfig {
    ///     temperature: Some(0.5),
    ///     top_p: Some(0.9),
    ///     ..Default::default()
    /// };
    ///
    /// request.merge_with(&model_defaults);
    /// assert_eq!(request.temperature, Some(0.8)); // Request value wins
    /// assert_eq!(request.top_p, Some(0.9));      // Fallback to model default
    /// ```
    pub fn merge_with(&mut self, other: &InferenceConfig) {
        if self.temperature.is_none() {
            self.temperature = other.temperature;
        }
        if self.top_p.is_none() {
            self.top_p = other.top_p;
        }
        if self.top_k.is_none() {
            self.top_k = other.top_k;
        }
        if self.max_tokens.is_none() {
            self.max_tokens = other.max_tokens;
        }
        if self.repeat_penalty.is_none() {
            self.repeat_penalty = other.repeat_penalty;
        }
    }

    /// Create a new config with all fields set to sensible defaults.
    ///
    /// These are the hardcoded fallback values used when no other
    /// defaults are configured.
    #[must_use]
    pub const fn with_hardcoded_defaults() -> Self {
        Self {
            temperature: Some(0.7),
            top_p: Some(0.95),
            top_k: Some(40),
            max_tokens: Some(2048),
            repeat_penalty: Some(1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_all_none() {
        let config = InferenceConfig::default();
        assert!(config.temperature.is_none());
        assert!(config.top_p.is_none());
        assert!(config.top_k.is_none());
        assert!(config.max_tokens.is_none());
        assert!(config.repeat_penalty.is_none());
    }

    #[test]
    fn test_merge_with_prefers_self() {
        let mut request = InferenceConfig {
            temperature: Some(0.8),
            top_p: None,
            ..Default::default()
        };

        let model_defaults = InferenceConfig {
            temperature: Some(0.5),
            top_p: Some(0.9),
            top_k: Some(50),
            ..Default::default()
        };

        request.merge_with(&model_defaults);

        assert_eq!(request.temperature, Some(0.8)); // Request wins
        assert_eq!(request.top_p, Some(0.9));       // Fallback to model
        assert_eq!(request.top_k, Some(50));        // Fallback to model
        assert!(request.max_tokens.is_none());      // Still None
    }

    #[test]
    fn test_hardcoded_defaults() {
        let config = InferenceConfig::with_hardcoded_defaults();
        assert_eq!(config.temperature, Some(0.7));
        assert_eq!(config.top_p, Some(0.95));
        assert_eq!(config.top_k, Some(40));
        assert_eq!(config.max_tokens, Some(2048));
        assert_eq!(config.repeat_penalty, Some(1.0));
    }

    #[test]
    fn test_serialization() {
        let config = InferenceConfig {
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: None,
            max_tokens: Some(1024),
            repeat_penalty: None,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: InferenceConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config, deserialized);
    }
}
