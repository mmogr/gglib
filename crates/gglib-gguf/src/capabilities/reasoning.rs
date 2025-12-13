//! Reasoning/thinking model capability detection.

use std::collections::HashMap;

use super::patterns::{
    REASONING_NAME_HIGH_CONFIDENCE, REASONING_NAME_MEDIUM_CONFIDENCE, THINKING_TAG_PATTERNS,
};

/// Result of reasoning capability detection.
#[derive(Debug, Clone, Default)]
pub struct ReasoningDetection {
    /// Whether the model appears to support reasoning/thinking.
    pub supports_reasoning: bool,
    /// Confidence level of the detection (0.0 to 1.0).
    pub confidence: f32,
    /// The specific pattern(s) that matched.
    pub matched_patterns: Vec<String>,
    /// Suggested reasoning format for llama-server.
    pub suggested_format: Option<String>,
}

/// Detect if a model supports reasoning/thinking based on its metadata.
///
/// Analyzes the chat template and model name to determine if the model
/// is a reasoning model that outputs `<think>` or similar tags.
#[must_use]
pub fn detect_reasoning_support(metadata: &HashMap<String, String>) -> ReasoningDetection {
    let mut detection = ReasoningDetection::default();
    let mut score = 0.0f32;

    // Check chat template for thinking patterns (highest confidence)
    if let Some(template) = metadata.get("tokenizer.chat_template") {
        let template_lower = template.to_lowercase();

        for pattern in THINKING_TAG_PATTERNS {
            let pattern_lower = pattern.to_lowercase();
            if template_lower.contains(&pattern_lower) {
                detection.matched_patterns.push((*pattern).to_string());
                // Opening tags are higher confidence
                if pattern.starts_with('<') && !pattern.starts_with("</") {
                    score += 0.4;
                } else {
                    score += 0.2;
                }
            }
        }

        // Check for template variables indicating thinking support
        if template_lower.contains("enable_thinking")
            || template_lower.contains("thinking_forced_open")
        {
            score += 0.3;
        }
    }

    // Check model name for reasoning patterns
    if let Some(name) = metadata.get("general.name") {
        let name_lower = name.to_lowercase();

        // High-confidence patterns
        for pattern in REASONING_NAME_HIGH_CONFIDENCE {
            if name_lower.contains(pattern) {
                detection.matched_patterns.push(format!("name:{pattern}"));
                score += 0.4;
            }
        }

        // Medium-confidence patterns (skip if already matched as high)
        for pattern in REASONING_NAME_MEDIUM_CONFIDENCE {
            if !REASONING_NAME_HIGH_CONFIDENCE.contains(pattern) && name_lower.contains(pattern) {
                detection.matched_patterns.push(format!("name:{pattern}"));
                score += 0.25;
            }
        }
    }

    // Check architecture
    if let Some(arch) = metadata.get("general.architecture") {
        if arch.to_lowercase().contains("deepseek") {
            score += 0.15;
            detection.matched_patterns.push(format!("arch:{arch}"));
        }
    }

    // Normalize and finalize
    detection.confidence = score.min(1.0);
    detection.supports_reasoning = detection.confidence >= 0.3;

    if detection.supports_reasoning {
        detection.suggested_format = Some("deepseek".to_string());
    }

    detection
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_think_tags() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "{% if message.role == 'assistant' %}<think>{{ message.thinking }}</think>{% endif %}"
                .to_string(),
        );

        let detection = detect_reasoning_support(&metadata);
        assert!(detection.supports_reasoning);
        assert!(detection.confidence >= 0.3);
        assert!(
            detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("think"))
        );
        assert_eq!(detection.suggested_format, Some("deepseek".to_string()));
    }

    #[test]
    fn test_detect_by_model_name() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "general.name".to_string(),
            "DeepSeek-R1-Distill-Qwen-32B".to_string(),
        );

        let detection = detect_reasoning_support(&metadata);
        assert!(detection.supports_reasoning);
        assert!(
            detection
                .matched_patterns
                .iter()
                .any(|p| p.contains("deepseek-r1"))
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

        let detection = detect_reasoning_support(&metadata);
        assert!(!detection.supports_reasoning);
        assert!(detection.confidence < 0.3);
    }
}
