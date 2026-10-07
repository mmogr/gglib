//! A [`NewModel`] through JSON and back.
//!
//! Exact float comparisons below check that a value survives a JSON
//! round-trip bit-for-bit, so `clippy::float_cmp` is intentionally allowed.
#![allow(clippy::float_cmp)]

use chrono::Utc;
use gglib_core::NewModel;
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
fn test_new_model_with_minimal_data() {
    let model = NewModel::new(
        String::new(),               // Empty name
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
