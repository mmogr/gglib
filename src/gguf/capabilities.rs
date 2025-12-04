//! Model capability detection (reasoning, tool calling).
//!
//! This module analyzes GGUF metadata to detect model capabilities
//! such as reasoning/thinking support and tool/function calling.

use std::collections::HashMap;

// =============================================================================
// Pattern Constants
// =============================================================================

/// Known thinking/reasoning tag patterns used by various models.
const THINKING_TAG_PATTERNS: &[&str] = &[
    // Standard patterns (DeepSeek R1, Qwen3, most reasoning models)
    "<think>",
    "<think ",
    "</think>",
    // Alternative tag names
    "<reasoning>",
    "</reasoning>",
    // Seed-OSS models
    "<seed:think>",
    "</seed:think>",
    // Command-R7B style
    "<|START_THINKING|>",
    "<|END_THINKING|>",
    // Apertus style
    "<|inner_prefix|>",
    "<|inner_suffix|>",
    // Nemotron V2 style
    "enable_thinking",
    // Bailing/Ring models
    "thinking_forced_open",
];

/// High-confidence reasoning model name patterns.
const REASONING_NAME_HIGH_CONFIDENCE: &[&str] = &["deepseek-r1", "qwq", "o1", "o3"];

/// Medium-confidence reasoning model name patterns.
const REASONING_NAME_MEDIUM_CONFIDENCE: &[&str] =
    &["deepseek-v3", "qwen3", "thinking", "reasoning", "cot"];

/// Tool calling model name patterns.
const TOOL_CALLING_NAME_PATTERNS: &[&str] = &[
    "hermes",
    "functionary",
    "firefunction",
    "toolcall",
    "function",
    "agent",
];

/// High-confidence tool calling patterns with format hints.
const TOOL_PATTERNS_HIGH_CONFIDENCE: &[(&str, &str, f32)] = &[
    ("<tool_call>", "hermes", 0.5),
    ("</tool_call>", "hermes", 0.3),
    ("<tool_response>", "hermes", 0.3),
    ("[tool_calls]", "mistral", 0.5),
    ("[tool_results]", "mistral", 0.3),
    ("<｜tool▁calls▁begin｜>", "deepseek", 0.5),
    ("<｜tool▁call▁begin｜>", "deepseek", 0.4),
    ("<|python_tag|>", "llama3", 0.5),
    ("functools[", "firefunction", 0.5),
    (">>>", "functionary", 0.3),
    ("from functions import", "functionary", 0.4),
];

/// Medium-confidence tool calling patterns (Jinja conditionals).
const TOOL_PATTERNS_MEDIUM_CONFIDENCE: &[&str] = &[
    "if tools",
    "tools is defined",
    "tools | length",
    "available_tools",
];

// =============================================================================
// Detection Result Types
// =============================================================================

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

// =============================================================================
// Detection Functions
// =============================================================================

/// Detect if a model supports reasoning/thinking based on its metadata.
///
/// Analyzes the chat template and model name to determine if the model
/// is a reasoning model that outputs `<think>` or similar tags.
pub fn detect_reasoning_support(metadata: &HashMap<String, String>) -> ReasoningDetection {
    let mut detection = ReasoningDetection::default();
    let mut score = 0.0f32;

    // Check chat template for thinking patterns (highest confidence)
    if let Some(template) = metadata.get("tokenizer.chat_template") {
        let template_lower = template.to_lowercase();

        for pattern in THINKING_TAG_PATTERNS {
            let pattern_lower = pattern.to_lowercase();
            if template_lower.contains(&pattern_lower) {
                detection.matched_patterns.push(pattern.to_string());
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
                detection.matched_patterns.push(format!("name:{}", pattern));
                score += 0.4;
            }
        }

        // Medium-confidence patterns (skip if already matched as high)
        for pattern in REASONING_NAME_MEDIUM_CONFIDENCE {
            if !REASONING_NAME_HIGH_CONFIDENCE.contains(pattern) && name_lower.contains(pattern) {
                detection.matched_patterns.push(format!("name:{}", pattern));
                score += 0.25;
            }
        }
    }

    // Check architecture
    if let Some(arch) = metadata.get("general.architecture") {
        if arch.to_lowercase().contains("deepseek") {
            score += 0.15;
            detection.matched_patterns.push(format!("arch:{}", arch));
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

/// Detect if a model supports tool/function calling based on its metadata.
///
/// Analyzes the chat template and model name to determine if the model
/// supports tool calling via the OpenAI-compatible API.
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
                detection.matched_patterns.push(pattern.to_string());
                score += pattern_score;
                if !detected_formats.contains(format) {
                    detected_formats.push(format);
                }
            }
        }

        // Medium-confidence patterns (Jinja conditionals)
        for pattern in TOOL_PATTERNS_MEDIUM_CONFIDENCE {
            if template_lower.contains(&pattern.to_lowercase()) {
                detection
                    .matched_patterns
                    .push(format!("jinja:{}", pattern));
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
                detection.matched_patterns.push(format!("name:{}", pattern));
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

/// Check if a model's metadata indicates it's a reasoning model.
///
/// Simplified boolean check for common use cases.
pub fn is_reasoning_model(metadata: &HashMap<String, String>) -> bool {
    detect_reasoning_support(metadata).supports_reasoning
}

/// Check if a model's metadata indicates it supports tool calling.
///
/// Simplified boolean check for common use cases.
pub fn is_tool_capable_model(metadata: &HashMap<String, String>) -> bool {
    detect_tool_support(metadata).supports_tool_calling
}

// =============================================================================
// Tag Application Functions
// =============================================================================

/// Apply reasoning detection and return tags to add.
///
/// Analyzes the metadata, logs detection results, and returns tags to apply.
pub fn apply_reasoning_detection(metadata: &HashMap<String, String>) -> Vec<String> {
    let detection = detect_reasoning_support(metadata);
    let mut tags = Vec::new();

    if detection.supports_reasoning {
        println!("\n🧠 Detected reasoning model capabilities:");
        println!("  Confidence: {:.0}%", detection.confidence * 100.0);
        if !detection.matched_patterns.is_empty() {
            println!(
                "  Matched patterns: {}",
                detection.matched_patterns.join(", ")
            );
        }
        if let Some(ref format) = detection.suggested_format {
            println!("  Suggested format: --reasoning-format {}", format);
        }
        println!("  → Auto-adding 'reasoning' tag for optimal llama-server configuration");
        tags.push("reasoning".to_string());
    }

    tags
}

/// Apply tool calling detection and return tags to add.
///
/// Returns "agent" tag (not "tools") because:
/// 1. "agent" triggers --jinja auto-enable
/// 2. More semantically accurate for agentic capabilities
pub fn apply_tool_detection(metadata: &HashMap<String, String>) -> Vec<String> {
    let detection = detect_tool_support(metadata);
    let mut tags = Vec::new();

    if detection.supports_tool_calling {
        println!("\n🔧 Detected tool calling capabilities:");
        println!("  Confidence: {:.0}%", detection.confidence * 100.0);
        if !detection.matched_patterns.is_empty() {
            println!(
                "  Matched patterns: {}",
                detection.matched_patterns.join(", ")
            );
        }
        if let Some(ref format) = detection.detected_format {
            println!("  Detected format: {}", format);
        }
        println!("  → Auto-adding 'agent' tag (enables --jinja for tool calling)");
        tags.push("agent".to_string());
    }

    tags
}

/// Apply both reasoning and tool calling detection, returning combined tags.
///
/// This is the recommended function for model import flows.
pub fn apply_capability_detection(metadata: &HashMap<String, String>) -> Vec<String> {
    let mut tags = Vec::new();

    for tag in apply_reasoning_detection(metadata) {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }

    for tag in apply_tool_detection(metadata) {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }

    tags
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod reasoning_tests {
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

    #[test]
    fn test_is_reasoning_model_helper() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<think>test</think>".to_string(),
        );
        assert!(is_reasoning_model(&metadata));

        let empty = HashMap::new();
        assert!(!is_reasoning_model(&empty));
    }
}

#[cfg(test)]
mod tool_calling_tests {
    use super::*;

    #[test]
    fn test_detect_hermes_tool_call_tags() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            r#"{% if tools %}<tool_call>{{ tool | tojson }}</tool_call>{% endif %}"#.to_string(),
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
    fn test_is_tool_capable_model_helper() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<tool_call>test</tool_call>".to_string(),
        );
        assert!(is_tool_capable_model(&metadata));

        let empty = HashMap::new();
        assert!(!is_tool_capable_model(&empty));
    }

    #[test]
    fn test_apply_tool_detection_adds_agent_tag() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<tool_call>test</tool_call>".to_string(),
        );

        let tags = apply_tool_detection(&metadata);
        assert!(tags.contains(&"agent".to_string()));
    }

    #[test]
    fn test_combined_capability_detection() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<think>reasoning</think><tool_call>tool</tool_call>".to_string(),
        );

        let tags = apply_capability_detection(&metadata);
        assert!(tags.contains(&"reasoning".to_string()));
        assert!(tags.contains(&"agent".to_string()));
    }

    #[test]
    fn test_capability_detection_no_duplicates() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<tool_call>tool</tool_call>".to_string(),
        );

        let tags = apply_capability_detection(&metadata);
        assert_eq!(tags.iter().filter(|t| *t == "agent").count(), 1);
    }
}
