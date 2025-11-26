//! Test fixtures for creating model data.
//!
//! Provides helper functions for creating test models with various configurations.

#![allow(dead_code)]

use chrono::Utc;
use gglib::models::Gguf;
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
pub fn create_test_model(name: &str) -> Gguf {
    create_test_model_with_params(name, 7.0)
}

/// Creates a test model with custom parameter count.
///
/// # Arguments
///
/// * `name` - The name for the test model
/// * `param_count` - The parameter count in billions
pub fn create_test_model_with_params(name: &str, param_count: f64) -> Gguf {
    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), name.to_string());
    metadata.insert("test_key".to_string(), "test_value".to_string());

    let mut model = Gguf::new(
        name.to_string(),
        PathBuf::from(format!("/test/{}.gguf", name)),
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
pub fn create_minimal_model(name: &str) -> Gguf {
    Gguf::new(
        name.to_string(),
        PathBuf::from(format!("/test/{}.gguf", name)),
        1.0,
        Utc::now(),
    )
}

/// Creates a test model with specific tags.
pub fn create_model_with_tags(name: &str, tags: Vec<String>) -> Gguf {
    let mut model = create_test_model(name);
    model.tags = tags;
    model
}

/// Creates a test model with custom metadata.
pub fn create_model_with_metadata(name: &str, metadata: HashMap<String, String>) -> Gguf {
    let mut model = create_test_model(name);
    model.metadata = metadata;
    model
}
