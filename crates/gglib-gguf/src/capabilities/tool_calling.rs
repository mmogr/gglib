//! Tool/function calling capability detection.

use std::collections::HashMap;

use gglib_core::ports::{
    ToolFormat, ToolSupportDetection, ToolSupportDetectionInput, ToolSupportDetectorPort,
};

use super::patterns::{
    TOOL_CALLING_NAME_PATTERNS, TOOL_PATTERNS_HIGH_CONFIDENCE, TOOL_PATTERNS_MEDIUM_CONFIDENCE,
};

/// Result of tool calling capability detection.
#[derive(Debug, Clone, Default)]
pub struct ToolCallingDetection {
    /// Whether the model appears to support tool/function calling.
    pub supports_tool_calling: bool,
    /// Confidence level of the detection (0.0 to 1.0).
    pub confidence: f32,
    /// The specific pattern(s) that matched.
    pub matched_patterns: Vec<String>,
    /// Detected tool calling format (e.g., "hermes", "llama3", "mistral").
    pub detected_format: Option<String>,
}

/// Detect if a model supports tool/function calling based on its metadata.
///
/// Analyzes the chat template and model name to determine if the model
/// supports tool calling via the OpenAI-compatible API.
#[must_use]
pub fn detect_tool_support(metadata: &HashMap<String, String>) -> ToolCallingDetection {
    let mut detection = ToolCallingDetection::default();
    let mut score = 0.0f32;
    let mut detected_formats: Vec<&str> = Vec::new();

    // Check chat template for tool calling patterns
    if let Some(template) = metadata.get("tokenizer.chat_template") {
        let template_lower = template.to_lowercase();

        // High-confidence patterns
        for (pattern, format, pattern_score) in TOOL_PATTERNS_HIGH_CONFIDENCE {
            if template_lower.contains(&pattern.to_lowercase()) {
                detection.matched_patterns.push((*pattern).to_string());
                score += pattern_score;
                if !detected_formats.contains(format) {
                    detected_formats.push(format);
                }
            }
        }

        // Medium-confidence patterns (Jinja conditionals)
        for pattern in TOOL_PATTERNS_MEDIUM_CONFIDENCE {
            if template_lower.contains(&pattern.to_lowercase()) {
                detection.matched_patterns.push(format!("jinja:{pattern}"));
                score += 0.35;
            }
        }

        // Low-confidence patterns
        if template_lower.contains("tool_call")
            && !detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("tool_call"))
        {
            detection.matched_patterns.push("tool_call".to_string());
            score += 0.2;
        }
        if template_lower.contains("function_call") {
            detection.matched_patterns.push("function_call".to_string());
            score += 0.2;
        }
    }

    // Check model name for tool calling patterns
    if let Some(name) = metadata.get("general.name") {
        let name_lower = name.to_lowercase();

        for pattern in TOOL_CALLING_NAME_PATTERNS {
            if name_lower.contains(pattern) {
                detection.matched_patterns.push(format!("name:{pattern}"));
                if *pattern == "hermes" || *pattern == "functionary" || *pattern == "firefunction" {
                    score += 0.4;
                    if *pattern == "hermes" && !detected_formats.contains(&"hermes") {
                        detected_formats.push("hermes");
                    }
                } else {
                    score += 0.25;
                }
            }
        }
    }

    // Normalize and finalize
    detection.confidence = score.min(1.0);
    detection.supports_tool_calling = detection.confidence >= 0.3;

    if !detected_formats.is_empty() {
        detection.detected_format = Some(detected_formats[0].to_string());
    }

    detection
}

/// Adapter that implements the core `ToolSupportDetectorPort`.
///
/// This wraps the existing `detect_tool_support` function to bridge
/// between the core port interface and the internal implementation.
pub struct ToolSupportDetector;

impl ToolSupportDetector {
    /// Create a new tool support detector.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for ToolSupportDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolSupportDetectorPort for ToolSupportDetector {
    fn detect(&self, input: ToolSupportDetectionInput<'_>) -> ToolSupportDetection {
        // Build metadata HashMap from input
        let mut metadata = HashMap::new();

        // Add chat template if present
        if let Some(template) = input.chat_template {
            metadata.insert("tokenizer.chat_template".to_string(), template.to_string());
        }

        // Add model ID as general.name for name-based detection
        metadata.insert("general.name".to_string(), input.model_id.to_string());

        // Tags could be added here if we extend pattern matching in the future
        // For now, the existing logic primarily uses chat_template and name

        // Call existing detection logic
        let internal_detection = detect_tool_support(&metadata);

        // Map to core types
        let detected_format = internal_detection
            .detected_format
            .as_deref()
            .map(|f| match f {
                "hermes" => ToolFormat::Hermes,
                "llama3" => ToolFormat::Llama3,
                "mistral" => ToolFormat::Mistral,
                "openai" | "openai-tools" => ToolFormat::OpenAiTools,
                _ => ToolFormat::Generic,
            });

        ToolSupportDetection {
            supports_tool_calling: internal_detection.supports_tool_calling,
            confidence: internal_detection.confidence,
            detected_format,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_hermes_tool_call_tags() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            r"{% if tools %}<tool_call>{{ tool | tojson }}</tool_call>{% endif %}".to_string(),
        );

        let detection = detect_tool_support(&metadata);
        assert!(detection.supports_tool_calling);
        assert!(detection.confidence >= 0.3);
        assert!(
            detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("tool_call"))
        );
        assert_eq!(detection.detected_format, Some("hermes".to_string()));
    }

    #[test]
    fn test_detect_by_model_name_hermes() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "general.name".to_string(),
            "NousResearch/Hermes-3-Llama-3.1-8B".to_string(),
        );

        let detection = detect_tool_support(&metadata);
        assert!(detection.supports_tool_calling);
        assert!(
            detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("hermes"))
        );
    }

    #[test]
    fn test_no_detection_for_regular_model() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "{% for message in messages %}{{ message.content }}{% endfor %}".to_string(),
        );
        metadata.insert("general.name".to_string(), "Llama-2-7B-Chat".to_string());

        let detection = detect_tool_support(&metadata);
        assert!(!detection.supports_tool_calling);
        assert!(detection.confidence < 0.3);
    }

    #[test]
    fn test_detect_jinja_tools_conditional() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "{% if tools is defined %}Use available tools{% endif %}".to_string(),
        );

        let detection = detect_tool_support(&metadata);
        assert!(detection.supports_tool_calling);
        assert!(
            detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("jinja"))
        );
    }
}
