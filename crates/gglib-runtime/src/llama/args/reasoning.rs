//! Reasoning format resolution for llama.cpp launches.

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

/// Detection result from GGUF metadata analysis.
///
/// This struct is passed in from the caller who has access to GGUF parsing.
/// This keeps gglib-runtime decoupled from GGUF parsing logic.
#[derive(Debug, Clone, Default)]
pub struct ReasoningDetection {
    /// Whether the model appears to support reasoning.
    pub supports_reasoning: bool,
    /// Suggested reasoning format if supported (e.g., "deepseek").
    pub suggested_format: Option<String>,
}

/// Determine the reasoning format for llama-server launches.
///
/// Reasoning models (e.g., `DeepSeek-R1`, `QwQ`) emit thinking content that can be
/// extracted to a separate `reasoning_content` field in the response. This requires
/// the `--reasoning-format` flag to be passed to llama-server.
///
/// Resolution order:
/// 1. Explicit value from user (highest priority)
/// 2. Model has "reasoning" tag
/// 3. GGUF metadata indicates reasoning support (passed via detection param)
/// 4. Default: no reasoning format
///
/// Explicit values:
/// - "none": Don't extract, keep thinking in content as <think>...</think> tags
/// - "deepseek": Extract thoughts to `reasoning_content` field
/// - "deepseek-legacy": Both tags in content AND populate `reasoning_content`
pub fn resolve_reasoning_format(
    explicit: Option<String>,
    tags: &[String],
) -> ReasoningFormatResolution {
    resolve_reasoning_format_with_detection(explicit, tags, None)
}

/// Extended version of `resolve_reasoning_format` that also uses pre-computed detection.
///
/// This is the comprehensive resolver that checks:
/// 1. Explicit user setting
/// 2. "reasoning" tag on the model
/// 3. Pre-computed GGUF metadata detection result
///
/// # Arguments
/// * `explicit` - User-specified reasoning format (if any)
/// * `tags` - Model tags from database
/// * `detection` - Pre-computed reasoning detection from GGUF metadata (if available)
pub fn resolve_reasoning_format_with_detection(
    explicit: Option<String>,
    tags: &[String],
    detection: Option<ReasoningDetection>,
) -> ReasoningFormatResolution {
    // 1. Check explicit setting first
    if let Some(format) = explicit {
        let format_lower = format.to_lowercase();
        if format_lower == "none" || format_lower == "deepseek" || format_lower == "deepseek-legacy"
        {
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

    // 3. Check pre-computed GGUF detection result
    if let Some(det) = detection
        && det.supports_reasoning
    {
        return ReasoningFormatResolution {
            format: det.suggested_format,
            source: ReasoningFormatSource::MetadataDetection,
        };
    }

    // 4. Default: no reasoning format
    ReasoningFormatResolution {
        format: None,
        source: ReasoningFormatSource::Default,
    }
}
