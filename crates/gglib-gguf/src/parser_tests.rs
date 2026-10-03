//! Tests for the metadata extraction in [`super`].

use super::*;

#[test]
fn test_parse_param_label() {
    // Regular models
    assert!((parse_param_label("7B").unwrap() - 7.0).abs() < f64::EPSILON);
    assert!((parse_param_label("13B").unwrap() - 13.0).abs() < f64::EPSILON);
    assert!((parse_param_label("70B").unwrap() - 70.0).abs() < f64::EPSILON);

    // MoE models - should return TOTAL parameters (N × M)
    assert!((parse_param_label("8x7B").unwrap() - 56.0).abs() < f64::EPSILON);
    assert!((parse_param_label("64x2.6B").unwrap() - 166.4).abs() < 0.01);
    assert!((parse_param_label("512x2.5B").unwrap() - 1280.0).abs() < 0.01);

    // Invalid
    assert!(parse_param_label("invalid").is_none());
}

#[test]
fn test_parse_param_from_filename() {
    assert!((parse_param_from_filename("llama-7b-chat.gguf").unwrap() - 7.0).abs() < f64::EPSILON);
    assert!((parse_param_from_filename("model-13B-q4.gguf").unwrap() - 13.0).abs() < f64::EPSILON);
    assert!(parse_param_from_filename("no-params.gguf").is_none());
}

#[test]
fn test_extract_context_length_dynamic() {
    // Test with architecture-specific key
    let mut raw = HashMap::new();
    raw.insert("llama.context_length".to_string(), GgufValue::U32(4096));
    let arch = Some("llama".to_string());
    assert_eq!(extract_context_length(&raw, arch.as_ref()), Some(4096));

    // Test with different architecture
    let mut raw2 = HashMap::new();
    raw2.insert("qwen2.context_length".to_string(), GgufValue::U64(32768));
    let arch2 = Some("qwen2".to_string());
    assert_eq!(extract_context_length(&raw2, arch2.as_ref()), Some(32768));

    // Test with new architectures (deepseek2, qwen3next)
    let mut raw3 = HashMap::new();
    raw3.insert(
        "deepseek2.context_length".to_string(),
        GgufValue::U64(131_072),
    );
    let arch3 = Some("deepseek2".to_string());
    assert_eq!(extract_context_length(&raw3, arch3.as_ref()), Some(131_072));

    let mut raw4 = HashMap::new();
    raw4.insert(
        "qwen3next.context_length".to_string(),
        GgufValue::U32(131_072),
    );
    let arch4 = Some("qwen3next".to_string());
    assert_eq!(extract_context_length(&raw4, arch4.as_ref()), Some(131_072));

    // Test generic fallback
    let mut raw_generic = HashMap::new();
    raw_generic.insert("context_length".to_string(), GgufValue::U32(2048));
    assert_eq!(extract_context_length(&raw_generic, None), Some(2048));
}

#[test]
fn test_extract_moe_metadata() {
    let mut raw = HashMap::new();
    raw.insert("deepseek2.expert_count".to_string(), GgufValue::U32(64));
    raw.insert("deepseek2.expert_used_count".to_string(), GgufValue::U32(4));
    raw.insert(
        "deepseek2.expert_shared_count".to_string(),
        GgufValue::U32(1),
    );

    let arch = Some("deepseek2".to_string());
    let (expert_count, expert_used_count, expert_shared_count) =
        extract_moe_metadata(&raw, arch.as_ref());

    assert_eq!(expert_count, Some(64));
    assert_eq!(expert_used_count, Some(4));
    assert_eq!(expert_shared_count, Some(1));

    // Test with no architecture
    let (count, used, shared) = extract_moe_metadata(&raw, None);
    assert_eq!(count, None);
    assert_eq!(used, None);
    assert_eq!(shared, None);
}

#[test]
fn test_extract_quantization_from_filename() {
    assert_eq!(
        extract_quantization_from_filename("model-Q4_K_M.gguf"),
        "Q4_K_M"
    );
    assert_eq!(extract_quantization_from_filename("model-f16.gguf"), "F16");
    assert_eq!(extract_quantization_from_filename("model.gguf"), "Unknown");
    // Unsloth "UD-" dynamic quants keep their prefix.
    assert_eq!(
        extract_quantization_from_filename("model-UD-Q4_K_M.gguf"),
        "UD-Q4_K_M"
    );
    // IQ-family quants are recognised.
    assert_eq!(
        extract_quantization_from_filename("model-IQ4_XS.gguf"),
        "IQ4_XS"
    );
}
