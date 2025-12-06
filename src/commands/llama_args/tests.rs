//! Unit tests for llama argument resolution.

use super::context::{ContextResolutionSource, resolve_context_size};
use super::jinja::{JinjaResolutionSource, resolve_jinja_flag};
use super::reasoning::{
    ReasoningFormatSource, resolve_reasoning_format, resolve_reasoning_format_with_metadata,
};
use std::collections::HashMap;

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
    let result =
        resolve_reasoning_format_with_metadata(Some("none".to_string()), &[], Some(&metadata));
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
    let result =
        resolve_reasoning_format_with_metadata(None, &["reasoning".to_string()], Some(&metadata));
    assert_eq!(result.format, Some("deepseek".to_string()));
    assert_eq!(result.source, ReasoningFormatSource::ReasoningTag);
}
