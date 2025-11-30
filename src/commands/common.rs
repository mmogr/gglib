//! Shared helpers for CLI commands.
//!
//! This module hosts reusable utilities for building llama.cpp
//! invocations so that multiple commands can stay DRY.

use anyhow::{Result, anyhow};

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
/// Auto-detection:
/// - If the model has a "reasoning" tag, uses "deepseek" format (extracts to reasoning_content)
/// - Otherwise, no reasoning format is passed (default llama-server behavior)
///
/// Explicit values:
/// - "none": Don't extract, keep thinking in content as <think>...</think> tags
/// - "deepseek": Extract thoughts to reasoning_content field
/// - "deepseek-legacy": Both tags in content AND populate reasoning_content
pub fn resolve_reasoning_format(
    explicit: Option<String>,
    tags: &[String],
) -> ReasoningFormatResolution {
    match explicit {
        Some(format) => {
            let format_lower = format.to_lowercase();
            if format_lower == "none" || format_lower == "deepseek" || format_lower == "deepseek-legacy" {
                ReasoningFormatResolution {
                    format: Some(format_lower),
                    source: ReasoningFormatSource::Explicit,
                }
            } else {
                // Invalid format, fall back to default
                ReasoningFormatResolution {
                    format: None,
                    source: ReasoningFormatSource::Default,
                }
            }
        }
        None => {
            if tags.iter().any(|tag| tag.eq_ignore_ascii_case("reasoning")) {
                ReasoningFormatResolution {
                    format: Some("deepseek".to_string()),
                    source: ReasoningFormatSource::ReasoningTag,
                }
            } else {
                ReasoningFormatResolution {
                    format: None,
                    source: ReasoningFormatSource::Default,
                }
            }
        }
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
    fn reasoning_rejects_invalid_format() {
        let result = resolve_reasoning_format(Some("invalid".to_string()), &[]);
        assert_eq!(result.format, None);
        assert_eq!(result.source, ReasoningFormatSource::Default);
    }
}
