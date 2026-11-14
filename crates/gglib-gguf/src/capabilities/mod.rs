#![doc = include_str!("README.md")]
mod embedding;
mod mtp;
mod patterns;
mod reasoning;

/// What real chat templates do with the loop guard's note (#1052).
#[cfg(test)]
#[path = "loop_guard_note_templates_tests.rs"]
mod loop_guard_note_templates_tests;

use std::collections::HashMap;

use gglib_core::GgufCapabilities;
use gglib_core::domain::gguf::CapabilityFlags;
use gglib_core::domain::{ModelCapabilities, infer_from_chat_template};

use embedding::detect_embedding_support;
use mtp::detect_mtp_support;
use reasoning::detect_reasoning_support;

/// Detect all capabilities from metadata.
///
/// This is the main entry point for capability detection, combining
/// reasoning and tool calling detection into a single `GgufCapabilities`.
/// Tool calling is core's guess ([`infer_from_chat_template`]), the one the
/// stored `SUPPORTS_TOOL_CALLS` flag takes at import, so the `agent` tag and
/// that flag agree.
#[must_use]
pub(crate) fn detect_all(metadata: &HashMap<String, String>) -> GgufCapabilities {
    let mut flags = CapabilityFlags::empty();

    // Detect reasoning support
    if detect_reasoning_support(metadata) {
        flags |= CapabilityFlags::REASONING;
    }

    // Detect tool calling support
    let guess = infer_from_chat_template(
        metadata.get("tokenizer.chat_template").map(String::as_str),
        metadata.get("general.name").map(String::as_str),
    );
    if guess.contains(ModelCapabilities::SUPPORTS_TOOL_CALLS) {
        flags |= CapabilityFlags::TOOL_CALLING;
    }

    // Detect MTP (Multi-Token Prediction) draft heads
    if detect_mtp_support(metadata) {
        flags |= CapabilityFlags::MTP;
    }

    // Detect an embedding model — decides `--embeddings` at launch
    if detect_embedding_support(metadata) {
        flags |= CapabilityFlags::EMBEDDING;
    }

    GgufCapabilities { flags }
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

    /// The `agent` tag is core's guess for each template and name: a tool
    /// template, a tool model's name, and a plain chat model.
    #[test]
    fn the_tool_flag_is_cores_guess() {
        let cases = [
            (
                Some("{% if tools %}<tool_call>{{ tool | tojson }}</tool_call>{% endif %}"),
                None,
                true,
            ),
            (None, Some("NousResearch/Hermes-3-Llama-3.1-8B"), true),
            (
                Some("{% for message in messages %}{{ message.content }}{% endfor %}"),
                Some("Llama-2-7B-Chat"),
                false,
            ),
        ];
        for (template, name, expected) in cases {
            let mut metadata = HashMap::new();
            if let Some(template) = template {
                metadata.insert("tokenizer.chat_template".to_string(), template.to_string());
            }
            if let Some(name) = name {
                metadata.insert("general.name".to_string(), name.to_string());
            }
            let guess = infer_from_chat_template(template, name)
                .contains(ModelCapabilities::SUPPORTS_TOOL_CALLS);
            assert_eq!(guess, expected, "core's guess for {template:?}, {name:?}");
            assert_eq!(
                detect_all(&metadata).has_tool_calling(),
                guess,
                "detect_all for {template:?}, {name:?}"
            );
        }
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
    fn test_detect_all_embedding() {
        let mut metadata = HashMap::new();
        metadata.insert("general.architecture".to_string(), "bert".to_string());
        metadata.insert("bert.pooling_type".to_string(), "1".to_string());

        let caps = detect_all(&metadata);
        assert!(caps.has_embedding());
        assert!(caps.to_tags().contains(&"embedding".to_string()));
    }

    #[test]
    fn a_model_with_mtp_layers_is_flagged_mtp_and_one_without_is_not() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "qwen3_5_mtp.nextn_predict_layers".to_string(),
            "1".to_string(),
        );

        assert!(detect_all(&metadata).has_mtp());
        assert!(!detect_all(&HashMap::new()).has_mtp());
    }

    #[test]
    fn test_detect_all_chat_model_is_not_embedding() {
        let mut metadata = HashMap::new();
        metadata.insert("general.architecture".to_string(), "qwen3".to_string());
        metadata.insert(
            "tokenizer.chat_template".to_string(),
            "<tool_call>test</tool_call>".to_string(),
        );

        let caps = detect_all(&metadata);
        assert!(!caps.has_embedding());
        assert!(!caps.to_tags().contains(&"embedding".to_string()));
    }
}
