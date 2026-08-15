#![doc = include_str!("README.md")]
mod embedding;
mod mtp;
mod patterns;
mod reasoning;
mod template_probe;
pub(crate) mod tool_calling;

use std::collections::HashMap;

use gglib_core::GgufCapabilities;
use gglib_core::domain::dialect::DialectSpec;
use gglib_core::domain::gguf::CapabilityFlags;

use embedding::detect_embedding_support;
use mtp::detect_mtp_support;
use reasoning::detect_reasoning_support;
use tool_calling::detect_tool_support;

/// Detect all capabilities from metadata.
///
/// This is the main entry point for capability detection, combining
/// reasoning and tool calling detection into a single `GgufCapabilities`.
#[must_use]
pub(crate) fn detect_all(metadata: &HashMap<String, String>) -> GgufCapabilities {
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

    // Detect MTP (Multi-Token Prediction) draft heads
    let mtp = detect_mtp_support(metadata);
    if mtp.supported {
        flags |= CapabilityFlags::MTP;
    }

    // Detect an embedding model — decides `--embeddings` at launch
    if detect_embedding_support(metadata) {
        flags |= CapabilityFlags::EMBEDDING;
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

    // Structured dialect spec, in precedence order: the template probe
    // (render-and-diff over the model's own chat template) first, then the
    // pattern-table result for the two formats the builtin spec covers.
    // Gated on tool-calling for the same reason as the format tag.
    let dialect = if tool_calling.supports_tool_calling {
        template_probe::derive(metadata).or_else(|| {
            // Literals match this module's own pattern-table vocabulary
            // (`patterns.rs` / the qwen override in `tool_calling.rs`).
            matches!(
                tool_calling.detected_format.as_deref(),
                Some("qwen-xml" | "hermes")
            )
            .then(DialectSpec::qwen_xml)
        })
    } else {
        None
    };

    GgufCapabilities {
        flags,
        extensions,
        dialect,
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
        // The template mentions the markers but renders no tool calls, so
        // the probe yields nothing — the pattern-table fallback still
        // resolves hermes to the builtin spec.
        assert_eq!(caps.dialect, Some(DialectSpec::qwen_xml()));
    }

    /// The probe path end to end: an executable template that renders
    /// tool calls produces a structured spec on the capabilities.
    #[test]
    fn test_detect_all_derives_dialect_from_executable_template() {
        let template = concat!(
            "{% for m in messages %}",
            "{% if m.tool_calls %}",
            "{% for tc in m.tool_calls %}",
            "[CALL]{\"name\": \"{{ tc.function.name }}\", \"arguments\": {{ tc.function.arguments | tojson }}}[/CALL]",
            "{% endfor %}",
            "{% else %}{{ m.content }}{% endif %}",
            "{% endfor %}",
        );
        let mut metadata = HashMap::new();
        metadata.insert("tokenizer.chat_template".to_string(), template.to_string());
        // The pattern tables recognise nothing here — tool-calling support
        // itself comes from the jinja conditional heuristics.
        metadata.insert("general.name".to_string(), "custom-agent-model".to_string());

        let caps = detect_all(&metadata);
        assert!(caps.has_tool_calling());
        let dialect = caps.dialect.expect("probe must derive a spec");
        assert_eq!(dialect.tool_open, "[CALL]");
        assert_eq!(dialect.tool_close, "[/CALL]");
    }

    /// No tool calling, no dialect — a stray spec on a non-tool-calling
    /// model would wire a parser with nothing to parse.
    #[test]
    fn test_detect_all_no_dialect_without_tool_calling() {
        let caps = detect_all(&HashMap::new());
        assert_eq!(caps.dialect, None);
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
    fn test_detect_all_embedding() {
        let mut metadata = HashMap::new();
        metadata.insert("general.architecture".to_string(), "bert".to_string());
        metadata.insert("bert.pooling_type".to_string(), "1".to_string());

        let caps = detect_all(&metadata);
        assert!(caps.has_embedding());
        assert!(caps.to_tags().contains(&"embedding".to_string()));
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
