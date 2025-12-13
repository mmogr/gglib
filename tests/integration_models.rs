//! Integration tests for model structures and serialization
//!
//! These tests verify the data models work correctly with various
//! data types, edge cases, and serialization scenarios.

use chrono::Utc;
use gglib_core::{GgufMetadata, NewModel};
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn test_new_model_serialization() {
    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), "Test Model".to_string());
    metadata.insert("general.architecture".to_string(), "llama".to_string());
    metadata.insert("llama.context_length".to_string(), "4096".to_string());

    let mut original_model = NewModel::new(
        "Test Llama Model".to_string(),
        PathBuf::from("/models/test-llama.gguf"),
        7.0,
        Utc::now(),
    );
    original_model.architecture = Some("llama".to_string());
    original_model.quantization = Some("Q4_0".to_string());
    original_model.context_length = Some(4096);
    original_model.metadata = metadata.clone();

    // Test JSON serialization
    let serialized = serde_json::to_string(&original_model).unwrap();
    assert!(serialized.contains("Test Llama Model"));
    assert!(serialized.contains("llama"));
    assert!(serialized.contains("Q4_0"));

    // Test JSON deserialization
    let deserialized: NewModel = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.name, original_model.name);
    assert_eq!(deserialized.param_count_b, original_model.param_count_b);
    assert_eq!(deserialized.architecture, original_model.architecture);
    assert_eq!(deserialized.quantization, original_model.quantization);
    assert_eq!(deserialized.context_length, original_model.context_length);
    assert_eq!(deserialized.metadata, original_model.metadata);
}

#[test]
fn test_new_model_with_extreme_values() {
    let mut model = NewModel::new(
        "Extreme Test Model".to_string(),
        PathBuf::from(
            "/extremely/long/path/to/a/model/file/that/has/many/directories/in/its/path/model.gguf",
        ),
        1750.0, // Very large model
        Utc::now(),
    );
    model.architecture = Some("custom_architecture_with_long_name".to_string());
    model.quantization = Some("CUSTOM_QUANT_TYPE".to_string());
    model.context_length = Some(1_000_000); // Very large context

    // Test with many metadata entries
    let mut m = HashMap::new();
    for i in 0..100 {
        m.insert(format!("key_{i}"), format!("value_{i}"));
    }
    model.metadata = m;

    // Should handle extreme values gracefully
    assert_eq!(model.param_count_b, 1750.0);
    assert_eq!(model.context_length, Some(1_000_000));
    assert_eq!(model.metadata.len(), 100);

    // Test serialization with extreme values
    let serialized = serde_json::to_string(&model).unwrap();
    let deserialized: NewModel = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.metadata.len(), 100);
    assert_eq!(deserialized.param_count_b, 1750.0);
}

#[test]
fn test_new_model_with_unicode_content() {
    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), "测试模型".to_string());
    metadata.insert(
        "description".to_string(),
        "Un modèle avec des caractères spéciaux 🦙".to_string(),
    );
    metadata.insert("emoji_test".to_string(), "🚀🤖🎯✨".to_string());

    let mut model = NewModel::new(
        "多语言模型 (Multilingual Model)".to_string(),
        PathBuf::from("/models/测试/模型.gguf"),
        7.0,
        Utc::now(),
    );
    model.architecture = Some("llama".to_string());
    model.quantization = Some("Q4_0".to_string());
    model.context_length = Some(4096);
    model.metadata = metadata;

    // Test that Unicode content is preserved
    assert!(model.name.contains("多语言"));
    assert_eq!(model.metadata.get("general.name").unwrap(), "测试模型");
    assert!(model.metadata.get("description").unwrap().contains("🦙"));

    // Test serialization preserves Unicode
    let serialized = serde_json::to_string(&model).unwrap();
    let deserialized: NewModel = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.name, model.name);
    assert_eq!(deserialized.metadata.get("emoji_test").unwrap(), "🚀🤖🎯✨");
}

#[test]
fn test_new_model_with_minimal_data() {
    let model = NewModel::new(
        "".to_string(),              // Empty name
        PathBuf::from("model.gguf"), // Minimal path
        0.0,                         // Zero parameters
        Utc::now(),
    );

    // Should handle minimal/empty values
    assert_eq!(model.name, "");
    assert_eq!(model.param_count_b, 0.0);
    assert_eq!(model.architecture, None);
    assert!(model.metadata.is_empty());

    // Test serialization with minimal data
    let serialized = serde_json::to_string(&model).unwrap();
    let deserialized: NewModel = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.name, "");
    assert_eq!(deserialized.param_count_b, 0.0);
}

#[test]
fn test_gguf_metadata_structure() {
    let mut metadata_map = HashMap::new();
    metadata_map.insert("general.name".to_string(), "Test Model".to_string());
    metadata_map.insert("general.architecture".to_string(), "llama".to_string());
    metadata_map.insert("llama.vocab_size".to_string(), "32000".to_string());

    let gguf_metadata = GgufMetadata {
        name: Some("Parsed Model Name".to_string()),
        architecture: Some("llama".to_string()),
        param_count_b: Some(7.0),
        quantization: Some("Q4_0".to_string()),
        context_length: Some(4096),
        metadata: metadata_map.clone(),
    };

    // Test all fields are accessible
    assert_eq!(gguf_metadata.name.unwrap(), "Parsed Model Name");
    assert_eq!(gguf_metadata.architecture.unwrap(), "llama");
    assert_eq!(gguf_metadata.param_count_b.unwrap(), 7.0);
    assert_eq!(gguf_metadata.quantization.unwrap(), "Q4_0");
    assert_eq!(gguf_metadata.context_length.unwrap(), 4096);
    assert_eq!(gguf_metadata.metadata.len(), 3);
}

#[test]
fn test_gguf_metadata_with_all_none_values() {
    let gguf_metadata = GgufMetadata {
        name: None,
        architecture: None,
        param_count_b: None,
        quantization: None,
        context_length: None,
        metadata: HashMap::new(),
    };

    // Should handle all None values gracefully
    assert_eq!(gguf_metadata.name, None);
    assert_eq!(gguf_metadata.architecture, None);
    assert_eq!(gguf_metadata.param_count_b, None);
    assert_eq!(gguf_metadata.quantization, None);
    assert_eq!(gguf_metadata.context_length, None);
    assert!(gguf_metadata.metadata.is_empty());
}

#[test]
fn test_datetime_handling() {
    let now = Utc::now();
    let model = NewModel::new(
        "DateTime Test".to_string(),
        PathBuf::from("/test/datetime.gguf"),
        1.0,
        now,
    );

    // Test datetime is preserved
    assert_eq!(model.added_at, now);

    // Test serialization preserves datetime
    let serialized = serde_json::to_string(&model).unwrap();
    let deserialized: NewModel = serde_json::from_str(&serialized).unwrap();

    // DateTime should be very close (within a few milliseconds)
    let time_diff = (deserialized.added_at - model.added_at)
        .num_milliseconds()
        .abs();
    assert!(
        time_diff < 1000,
        "DateTime should be preserved in serialization"
    );
}

#[test]
fn test_path_handling() {
    let test_paths = vec![
        "/simple/path/model.gguf",
        "/path/with spaces/model.gguf",
        "/path/with-dashes/and_underscores/model.gguf",
        "C:\\Windows\\Path\\model.gguf", // Windows-style path
        "/très/long/chemin/avec/caractères/spéciaux/模型.gguf", // Unicode path
    ];

    for path_str in test_paths {
        let model = NewModel::new(
            format!("Test for {path_str}"),
            PathBuf::from(path_str),
            1.0,
            Utc::now(),
        );

        // Test path is preserved
        assert_eq!(model.file_path.to_string_lossy(), path_str);

        // Test serialization preserves path
        let serialized = serde_json::to_string(&model).unwrap();
        let deserialized: NewModel = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.file_path.to_string_lossy(), path_str);
    }
}

#[test]
fn test_metadata_edge_cases() {
    let mut complex_metadata = HashMap::new();

    // Test various edge cases in metadata
    complex_metadata.insert("empty_value".to_string(), "".to_string());
    complex_metadata.insert(
        "very.long.nested.key.with.many.dots".to_string(),
        "nested_value".to_string(),
    );
    complex_metadata.insert(
        "key with spaces".to_string(),
        "value with spaces".to_string(),
    );
    complex_metadata.insert("unicode_key_🔑".to_string(), "unicode_value_🎯".to_string());
    complex_metadata.insert(
        "JSON_like".to_string(),
        r#"{"nested": "json", "array": [1,2,3]}"#.to_string(),
    );
    complex_metadata.insert("multiline".to_string(), "line1\nline2\nline3".to_string());
    complex_metadata.insert(
        "special_chars".to_string(),
        "!@#$%^&*()[]{}|\\:;\"'<>,.?/~`".to_string(),
    );

    let mut model = NewModel::new(
        "Metadata Edge Cases Test".to_string(),
        PathBuf::from("/test/metadata_edges.gguf"),
        1.0,
        Utc::now(),
    );
    model.metadata = complex_metadata.clone();

    // Test all edge case metadata is preserved
    assert_eq!(model.metadata.len(), 7);
    assert_eq!(model.metadata.get("empty_value").unwrap(), "");
    assert!(model.metadata.get("unicode_key_🔑").unwrap().contains("🎯"));
    assert!(model.metadata.get("multiline").unwrap().contains("\n"));

    // Test serialization preserves complex metadata
    let serialized = serde_json::to_string(&model).unwrap();
    let deserialized: NewModel = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.metadata.len(), 7);
    assert_eq!(
        deserialized.metadata.get("special_chars").unwrap(),
        "!@#$%^&*()[]{}|\\:;\"'<>,.?/~`"
    );
}

#[test]
fn test_model_cloning() {
    let mut metadata = HashMap::new();
    metadata.insert("test".to_string(), "value".to_string());

    let mut original = NewModel::new(
        "Original Model".to_string(),
        PathBuf::from("/test/original.gguf"),
        7.0,
        Utc::now(),
    );
    original.architecture = Some("llama".to_string());
    original.quantization = Some("Q4_0".to_string());
    original.context_length = Some(4096);
    original.metadata = metadata;

    // NewModel should be cloneable
    let cloned = original.clone();
    assert_eq!(cloned.name, original.name);
    assert_eq!(cloned.architecture, original.architecture);

    // GgufMetadata should be cloneable
    let gguf_metadata = GgufMetadata {
        name: Some("Test".to_string()),
        architecture: Some("llama".to_string()),
        param_count_b: Some(7.0),
        quantization: Some("Q4_0".to_string()),
        context_length: Some(4096),
        metadata: HashMap::new(),
    };

    let cloned_metadata = gguf_metadata.clone();
    assert_eq!(cloned_metadata.name, gguf_metadata.name);
    assert_eq!(cloned_metadata.architecture, gguf_metadata.architecture);
}
