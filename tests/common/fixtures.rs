//! Test fixtures for creating model data.
//!
//! Provides helper functions for creating test models with various configurations.

#![allow(dead_code)]

use chrono::Utc;
use gglib_core::NewModel;
use std::collections::HashMap;
use std::path::PathBuf;

/// Creates a basic test model with default values.
///
/// # Arguments
///
/// * `name` - The name for the test model
///
/// # Example
///
/// ```rust,ignore
/// use crate::common::fixtures::create_test_model;
///
/// let model = create_test_model("my-test-model");
/// assert_eq!(model.name, "my-test-model");
/// ```
pub fn create_test_model(name: &str) -> NewModel {
    create_test_model_with_params(name, 7.0)
}

/// Creates a test model with custom parameter count.
///
/// # Arguments
///
/// * `name` - The name for the test model
/// * `param_count` - The parameter count in billions
pub fn create_test_model_with_params(name: &str, param_count: f64) -> NewModel {
    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), name.to_string());
    metadata.insert("test_key".to_string(), "test_value".to_string());

    let mut model = NewModel::new(
        name.to_string(),
        PathBuf::from(format!("/test/{name}.gguf")),
        param_count,
        Utc::now(),
    );
    model.architecture = Some("llama".to_string());
    model.quantization = Some("Q4_0".to_string());
    model.context_length = Some(4096);
    model.metadata = metadata;
    model
}

/// Creates a minimal test model with only required fields.
pub fn create_minimal_model(name: &str) -> NewModel {
    NewModel::new(
        name.to_string(),
        PathBuf::from(format!("/test/{name}.gguf")),
        1.0,
        Utc::now(),
    )
}

/// Creates a test model with specific tags.
pub fn create_model_with_tags(name: &str, tags: Vec<String>) -> NewModel {
    let mut model = create_test_model(name);
    model.tags = tags;
    model
}

/// Creates a test model with custom metadata.
pub fn create_model_with_metadata(name: &str, metadata: HashMap<String, String>) -> NewModel {
    let mut model = create_test_model(name);
    model.metadata = metadata;
    model
}

/// Creates a model with complex metadata for testing edge cases.
pub fn create_complex_metadata_model(name: &str) -> NewModel {
    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), name.to_string());
    metadata.insert(
        "general.description".to_string(),
        "A complex test model with extensive metadata".to_string(),
    );
    metadata.insert("general.author".to_string(), "Test Suite".to_string());
    metadata.insert("general.version".to_string(), "1.0.0".to_string());
    metadata.insert("llama.vocab_size".to_string(), "32000".to_string());
    metadata.insert("llama.attention.head_count".to_string(), "32".to_string());
    metadata.insert(
        "llama.attention.head_count_kv".to_string(),
        "32".to_string(),
    );
    metadata.insert("unicode_test".to_string(), "测试数据 🦙 émojis".to_string());
    metadata.insert(
        "special_chars".to_string(),
        "!@#$%^&*()[]{}|\\:;\"'<>,.?/~`".to_string(),
    );

    let mut model = NewModel::new(
        name.to_string(),
        PathBuf::from(format!("/test/complex/{name}.gguf")),
        7.0,
        Utc::now(),
    );
    model.architecture = Some("llama".to_string());
    model.quantization = Some("Q4_K_M".to_string());
    model.context_length = Some(8192);
    model.metadata = metadata;
    model
}

/// Assert that two models are equal, ignoring the `added_at` timestamp.
pub fn assert_models_equal_ignore_timestamp(model1: &NewModel, model2: &NewModel) {
    assert_eq!(model1.name, model2.name);
    assert_eq!(model1.file_path, model2.file_path);
    assert_eq!(model1.param_count_b, model2.param_count_b);
    assert_eq!(model1.architecture, model2.architecture);
    assert_eq!(model1.quantization, model2.quantization);
    assert_eq!(model1.context_length, model2.context_length);
    assert_eq!(model1.metadata, model2.metadata);
}
