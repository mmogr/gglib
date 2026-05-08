//! Model capability detection.
//!
//! This module analyzes GGUF metadata to detect model capabilities
//! such as reasoning/thinking support and tool/function calling.
//!
//! # Structure
//!
//! - `reasoning` - Reasoning/thinking model detection
//! - `tool_calling` - Tool/function calling detection
//! - `patterns` - Pattern constants shared across detection modules

mod patterns;
mod reasoning;
pub mod tool_calling;

use std::collections::HashMap;

use gglib_core::GgufCapabilities;
use gglib_core::domain::gguf::CapabilityFlags;

use reasoning::detect_reasoning_support;
use tool_calling::detect_tool_support;

/// Detect all capabilities from metadata.
///
/// This is the main entry point for capability detection, combining
/// reasoning and tool calling detection into a single `GgufCapabilities`.
#[must_use]
pub fn detect_all(metadata: &HashMap<String, String>) -> GgufCapabilities {
    let mut flags = CapabilityFlags::empty();

    // Detect reasoning support
    let reasoning = detect_reasoning_support(metadata);
    if reasoning.supports_reasoning {
        flags |= CapabilityFlags::REASONING;
    }

    // Detect tool calling support
    let tool_calling = detect_tool_support(metadata);
    if tool_calling.supports_tool_calling {
        flags |= CapabilityFlags::TOOL_CALLING;
    }

    // Surface the detected dialect as a `format:*` extension tag so the
    // normalization pipeline can pick a parser without re-deriving the
    // detection at runtime.  Only emit when tool-calling is actually
    // supported — a stray format hint on a non-tool-calling model would
    // wire a parser that has nothing to parse.
    let mut extensions = std::collections::BTreeSet::new();
    if tool_calling.supports_tool_calling
        && let Some(fmt) = tool_calling.detected_format.as_deref()
    {
        extensions.insert(format!("format:{fmt}"));
    }

    GgufCapabilities { flags, extensions }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_all_empty() {
        let metadata = HashMap::new();
        let caps = detect_all(&metadata);
        assert!(!caps.has_reasoning());
        assert!(!caps.has_tool_calling());
    }

    #[test]
    fn test_detect_all_reasoning() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<think>test</think>".to_string(),
        );

        let caps = detect_all(&metadata);
        assert!(caps.has_reasoning());
        assert!(caps.to_tags().contains(&"reasoning".to_string()));
    }

    #[test]
    fn test_detect_all_tool_calling() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<tool_call>test</tool_call>".to_string(),
        );

        let caps = detect_all(&metadata);
        assert!(caps.has_tool_calling());
        assert!(caps.to_tags().contains(&"agent".to_string()));
    }

    #[test]
    fn test_detect_all_combined() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<think>reasoning</think><tool_call>tool</tool_call>".to_string(),
        );

        let caps = detect_all(&metadata);
        assert!(caps.has_reasoning());
        assert!(caps.has_tool_calling());

        let tags = caps.to_tags();
        assert!(tags.contains(&"reasoning".to_string()));
        assert!(tags.contains(&"agent".to_string()));
    }

    #[test]
    fn test_detect_all_emits_hermes_format_tag() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<tool_call>{}</tool_call>".to_string(),
        );

        let caps = detect_all(&metadata);
        assert!(caps.has_tool_calling());
        assert!(caps.to_tags().contains(&"format:hermes".to_string()));
    }

    #[test]
    fn test_detect_all_emits_qwen_xml_format_tag() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<tool_call>{}</tool_call>".to_string(),
        );
        metadata.insert(
            "general.name".to_string(),
            "Qwen/Qwen2.5-7B-Instruct".to_string(),
        );

        let caps = detect_all(&metadata);
        assert!(caps.has_tool_calling());
        let tags = caps.to_tags();
        assert!(
            tags.contains(&"format:qwen-xml".to_string()),
            "expected format:qwen-xml in {tags:?}"
        );
        assert!(
            !tags.contains(&"format:hermes".to_string()),
            "qwen override should suppress hermes default in {tags:?}"
        );
    }

    #[test]
    fn test_detect_all_no_format_tag_without_tools() {
        let metadata = HashMap::new();
        let caps = detect_all(&metadata);
        assert!(!caps.has_tool_calling());
        assert!(
            caps.to_tags().iter().all(|t| !t.starts_with("format:")),
            "no format:* tag should be emitted when tool-calling is absent"
        );
    }
}
