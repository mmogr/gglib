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

    GgufCapabilities {
        flags,
        extensions: std::collections::BTreeSet::new(),
    }
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
}
