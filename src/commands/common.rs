//! Shared helpers for CLI commands.
//!
//! This module hosts reusable utilities for building llama.cpp
//! invocations so that multiple commands can stay DRY.

use std::collections::HashMap;
use anyhow::{Result, anyhow};
use crate::utils::gguf_parser;

/// Indicates how the Jinja flag was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JinjaResolutionSource {
    /// User explicitly forced Jinja on via CLI/UI flag.
    ExplicitTrue,
    /// User explicitly disabled Jinja even if tags would auto-enable it.
    ExplicitFalse,
    /// Auto-enabled because the model has the "agent" tag.
    AgentTag,
    /// Not enabled (default).
    Default,
}

/// Result of resolving whether to enable Jinja templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JinjaResolution {
    /// Whether the `--jinja` flag should be forwarded to llama.cpp.
    pub enabled: bool,
    /// Source of the decision, used for UX/logging.
    pub source: JinjaResolutionSource,
}

/// Indicates how a context size value was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextResolutionSource {
    /// User passed an explicit numeric flag.
    ExplicitFlag,
    /// User asked for `max` and we used the model metadata.
    ModelMetadata,
    /// The flag was omitted entirely.
    NotSpecified,
    /// User asked for `max` but the metadata did not contain a value.
    MaxRequestedMissing,
}

/// Result of resolving a context size flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextResolution {
    /// The numeric value to forward to llama.cpp (if any).
    pub value: Option<u32>,
    /// Indicates where the value came from for logging UX.
    pub source: ContextResolutionSource,
}

/// Normalize a context-size input ("max" or numeric string).
pub fn resolve_context_size(
    ctx_flag: Option<String>,
    model_context_length: Option<u64>,
) -> Result<ContextResolution> {
    match ctx_flag {
        Some(raw) => {
            let value = raw.trim();
            if value.eq_ignore_ascii_case("max") {
                if let Some(model_ctx) = model_context_length {
                    let ctx_u32 = u32::try_from(model_ctx).map_err(|_| {
                        anyhow!(
                            "Model context length {} exceeds supported range for llama.cpp",
                            model_ctx
                        )
                    })?;
                    Ok(ContextResolution {
                        value: Some(ctx_u32),
                        source: ContextResolutionSource::ModelMetadata,
                    })
                } else {
                    Ok(ContextResolution {
                        value: None,
                        source: ContextResolutionSource::MaxRequestedMissing,
                    })
                }
            } else {
                let ctx_value: u32 = value.parse().map_err(|_| {
                    anyhow!(
                        "Invalid context size '{}'. Use a positive number or 'max'",
                        value
                    )
                })?;

                Ok(ContextResolution {
                    value: Some(ctx_value),
                    source: ContextResolutionSource::ExplicitFlag,
                })
            }
        }
        None => Ok(ContextResolution {
            value: None,
            source: ContextResolutionSource::NotSpecified,
        }),
    }
}

/// Determine whether to enable Jinja templates for llama-server launches.
pub fn resolve_jinja_flag(explicit: Option<bool>, tags: &[String]) -> JinjaResolution {
    match explicit {
        Some(true) => JinjaResolution {
            enabled: true,
            source: JinjaResolutionSource::ExplicitTrue,
        },
        Some(false) => JinjaResolution {
            enabled: false,
            source: JinjaResolutionSource::ExplicitFalse,
        },
        None => {
            if tags.iter().any(|tag| tag.eq_ignore_ascii_case("agent")) {
                JinjaResolution {
                    enabled: true,
                    source: JinjaResolutionSource::AgentTag,
                }
            } else {
                JinjaResolution {
                    enabled: false,
                    source: JinjaResolutionSource::Default,
                }
            }
        }
    }
}

/// Indicates how the reasoning format was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningFormatSource {
    /// User explicitly set a reasoning format.
    Explicit,
    /// Auto-enabled because the model has a "reasoning" tag.
    ReasoningTag,
    /// Auto-enabled because GGUF metadata indicates reasoning support.
    MetadataDetection,
    /// Not enabled (default).
    Default,
}

/// Result of resolving the reasoning format for llama-server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasoningFormatResolution {
    /// The reasoning format to pass to llama-server (None = don't pass the flag).
    /// Valid values: "none", "deepseek", "deepseek-legacy"
    pub format: Option<String>,
    /// Source of the decision, used for UX/logging.
    pub source: ReasoningFormatSource,
}

/// Determine the reasoning format for llama-server launches.
///
/// Reasoning models (e.g., DeepSeek-R1, QwQ) emit thinking content that can be
/// extracted to a separate `reasoning_content` field in the response. This requires
/// the `--reasoning-format` flag to be passed to llama-server.
///
/// Resolution order:
/// 1. Explicit value from user (highest priority)
/// 2. Model has "reasoning" tag
/// 3. GGUF metadata indicates reasoning support (chat template contains <think> etc.)
/// 4. Default: no reasoning format
///
/// Explicit values:
/// - "none": Don't extract, keep thinking in content as <think>...</think> tags
/// - "deepseek": Extract thoughts to reasoning_content field
/// - "deepseek-legacy": Both tags in content AND populate reasoning_content
pub fn resolve_reasoning_format(
    explicit: Option<String>,
    tags: &[String],
) -> ReasoningFormatResolution {
    resolve_reasoning_format_with_metadata(explicit, tags, None)
}

/// Extended version of resolve_reasoning_format that also checks GGUF metadata.
///
/// This is the comprehensive resolver that checks:
/// 1. Explicit user setting
/// 2. "reasoning" tag on the model
/// 3. GGUF metadata (chat_template containing <think> patterns)
///
/// Use this when you have access to the model's metadata HashMap.
pub fn resolve_reasoning_format_with_metadata(
    explicit: Option<String>,
    tags: &[String],
    metadata: Option<&HashMap<String, String>>,
) -> ReasoningFormatResolution {
    // 1. Check explicit setting first
    if let Some(format) = explicit {
        let format_lower = format.to_lowercase();
        if format_lower == "none" || format_lower == "deepseek" || format_lower == "deepseek-legacy" {
            return ReasoningFormatResolution {
                format: Some(format_lower),
                source: ReasoningFormatSource::Explicit,
            };
        }
        // Invalid format, continue to other checks
    }

    // 2. Check for "reasoning" tag
    if tags.iter().any(|tag| tag.eq_ignore_ascii_case("reasoning")) {
        return ReasoningFormatResolution {
            format: Some("deepseek".to_string()),
            source: ReasoningFormatSource::ReasoningTag,
        };
    }

    // 3. Check GGUF metadata for reasoning patterns
    if let Some(meta) = metadata {
        let detection = gguf_parser::detect_reasoning_support(meta);
        if detection.supports_reasoning {
            return ReasoningFormatResolution {
                format: detection.suggested_format,
                source: ReasoningFormatSource::MetadataDetection,
            };
        }
    }

    // 4. Default: no reasoning format
    ReasoningFormatResolution {
        format: None,
        source: ReasoningFormatSource::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_numeric_flag() {
        let result = resolve_context_size(Some("4096".into()), Some(2048)).unwrap();
        assert_eq!(result.value, Some(4096));
        assert_eq!(result.source, ContextResolutionSource::ExplicitFlag);
    }

    #[test]
    fn resolves_max_with_model_metadata() {
        let result = resolve_context_size(Some("max".into()), Some(16384)).unwrap();
        assert_eq!(result.value, Some(16384));
        assert_eq!(result.source, ContextResolutionSource::ModelMetadata);
    }

    #[test]
    fn warns_when_max_missing() {
        let result = resolve_context_size(Some("max".into()), None).unwrap();
        assert_eq!(result.value, None);
        assert_eq!(result.source, ContextResolutionSource::MaxRequestedMissing);
    }

    #[test]
    fn handles_missing_flag() {
        let result = resolve_context_size(None, Some(4096)).unwrap();
        assert_eq!(result.value, None);
        assert_eq!(result.source, ContextResolutionSource::NotSpecified);
    }

    #[test]
    fn rejects_invalid_numbers() {
        let result = resolve_context_size(Some("abc".into()), None);
        assert!(result.is_err());
    }

    #[test]
    fn resolves_explicit_jinja_true() {
        let result = resolve_jinja_flag(Some(true), &[]);
        assert!(result.enabled);
        assert_eq!(result.source, JinjaResolutionSource::ExplicitTrue);
    }

    #[test]
    fn resolves_explicit_jinja_false() {
        let result = resolve_jinja_flag(Some(false), &[]);
        assert!(!result.enabled);
        assert_eq!(result.source, JinjaResolutionSource::ExplicitFalse);
    }

    #[test]
    fn auto_enables_for_agent_tag() {
        let tags = vec!["Agent".to_string(), "other".to_string()];
        let result = resolve_jinja_flag(None, &tags);
        assert!(result.enabled);
        assert_eq!(result.source, JinjaResolutionSource::AgentTag);
    }

    #[test]
    fn defaults_to_disabled() {
        let result = resolve_jinja_flag(None, &[]);
        assert!(!result.enabled);
        assert_eq!(result.source, JinjaResolutionSource::Default);
    }

    #[test]
    fn resolves_explicit_reasoning_format() {
        let result = resolve_reasoning_format(Some("deepseek".to_string()), &[]);
        assert_eq!(result.format, Some("deepseek".to_string()));
        assert_eq!(result.source, ReasoningFormatSource::Explicit);
    }

    #[test]
    fn auto_enables_reasoning_for_tag() {
        let tags = vec!["reasoning".to_string(), "other".to_string()];
        let result = resolve_reasoning_format(None, &tags);
        assert_eq!(result.format, Some("deepseek".to_string()));
        assert_eq!(result.source, ReasoningFormatSource::ReasoningTag);
    }

    #[test]
    fn reasoning_defaults_to_none() {
        let result = resolve_reasoning_format(None, &[]);
        assert_eq!(result.format, None);
        assert_eq!(result.source, ReasoningFormatSource::Default);
    }

    #[test]
    fn reasoning_from_metadata_detection() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "{% if message.role == 'assistant' %}<think>...</think>{% endif %}".to_string(),
        );
        
        let result = resolve_reasoning_format_with_metadata(None, &[], Some(&metadata));
        assert_eq!(result.format, Some("deepseek".to_string()));
        assert_eq!(result.source, ReasoningFormatSource::MetadataDetection);
    }

    #[test]
    fn explicit_overrides_metadata_detection() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<think>...</think>".to_string(),
        );
        
        // Explicit "none" should override metadata detection
        let result = resolve_reasoning_format_with_metadata(
            Some("none".to_string()),
            &[],
            Some(&metadata)
        );
        assert_eq!(result.format, Some("none".to_string()));
        assert_eq!(result.source, ReasoningFormatSource::Explicit);
    }

    #[test]
    fn tag_overrides_metadata_detection() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<think>...</think>".to_string(),
        );
        
        // Tag should take precedence over metadata (both should result in deepseek anyway)
        let result = resolve_reasoning_format_with_metadata(
            None,
            &["reasoning".to_string()],
            Some(&metadata)
        );
        assert_eq!(result.format, Some("deepseek".to_string()));
        assert_eq!(result.source, ReasoningFormatSource::ReasoningTag);
    }
}
