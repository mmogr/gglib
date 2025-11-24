//! Test utilities and helpers for the gglib project
//!
//! This module provides common test utilities, fixtures, and helper functions
//! that can be shared across different test modules.

use chrono::Utc;
use gglib::{models::Gguf, services::database};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;

/// Create a test database with a unique temporary file
pub async fn create_test_database() -> anyhow::Result<SqlitePool> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;

    // Create the table schema with enhanced metadata fields
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS models (
            id INTEGER PRIMARY KEY, 
            name TEXT NOT NULL, 
            file_path TEXT NOT NULL, 
            param_count_b REAL NOT NULL,
            architecture TEXT,
            quantization TEXT,
            context_length INTEGER,
            metadata TEXT,
            added_at TEXT NOT NULL,
            hf_repo_id TEXT,
            hf_commit_sha TEXT,
            hf_filename TEXT,
            download_date TEXT,
            last_update_check TEXT,
            tags TEXT NOT NULL DEFAULT '[]'
            )",
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

/// Create a test model with default values
pub fn create_test_model(name: &str) -> Gguf {
    create_test_model_with_params(name, 7.0, Some("llama"), Some("Q4_0"))
}

/// Create a test model with custom parameters
pub fn create_test_model_with_params(
    name: &str,
    param_count: f64,
    architecture: Option<&str>,
    quantization: Option<&str>,
) -> Gguf {
    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), name.to_string());
    metadata.insert("test.created_by".to_string(), "test_helper".to_string());

    Gguf {
        id: None,
        name: name.to_string(),
        file_path: PathBuf::from(format!("/test/models/{}.gguf", name)),
        param_count_b: param_count,
        architecture: architecture.map(|s| s.to_string()),
        quantization: quantization.map(|s| s.to_string()),
        context_length: Some(4096),
        metadata,
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    }
}

/// Setup a test database with sample models
pub async fn setup_test_database_with_models() -> anyhow::Result<SqlitePool> {
    let pool = create_test_database().await?;

    let models = vec![
        create_test_model("llama-7b-chat"),
        create_test_model_with_params("llama-13b-instruct", 13.0, Some("llama"), Some("Q4_K_M")),
        create_test_model_with_params("mistral-7b", 7.0, Some("mistral"), Some("Q4_0")),
        create_test_model_with_params("codellama-34b", 34.0, Some("llama"), Some("Q8_0")),
    ];

    for model in &models {
        database::add_model(&pool, model).await?;
    }

    Ok(pool)
}

/// Assert that two models are equal, ignoring the added_at timestamp
pub fn assert_models_equal_ignore_timestamp(model1: &Gguf, model2: &Gguf) {
    assert_eq!(model1.name, model2.name);
    assert_eq!(model1.file_path, model2.file_path);
    assert_eq!(model1.param_count_b, model2.param_count_b);
    assert_eq!(model1.architecture, model2.architecture);
    assert_eq!(model1.quantization, model2.quantization);
    assert_eq!(model1.context_length, model2.context_length);
    assert_eq!(model1.metadata, model2.metadata);
}

/// Create a model with complex metadata for testing edge cases
pub fn create_complex_metadata_model(name: &str) -> Gguf {
    let mut metadata = HashMap::new();

    // Add various types of metadata
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
    metadata.insert(
        "llama.attention.layer_norm_rms_epsilon".to_string(),
        "1e-06".to_string(),
    );
    metadata.insert("llama.rope.dimension_count".to_string(), "128".to_string());
    metadata.insert("unicode_test".to_string(), "测试数据 🦙 émojis".to_string());
    metadata.insert(
        "special_chars".to_string(),
        "!@#$%^&*()[]{}|\\:;\"'<>,.?/~`".to_string(),
    );

    Gguf {
        id: None,
        name: name.to_string(),
        file_path: PathBuf::from(format!("/test/complex/{}.gguf", name)),
        param_count_b: 7.0,
        architecture: Some("llama".to_string()),
        quantization: Some("Q4_K_M".to_string()),
        context_length: Some(8192),
        metadata,
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_test_database() {
        let pool = create_test_database().await;
        assert!(pool.is_ok());
    }

    #[test]
    fn test_create_test_model() {
        let model = create_test_model("test-model");
        assert_eq!(model.name, "test-model");
        assert_eq!(model.param_count_b, 7.0);
        assert_eq!(model.architecture, Some("llama".to_string()));
        assert_eq!(model.quantization, Some("Q4_0".to_string()));
    }

    #[test]
    fn test_create_test_model_with_params() {
        let model =
            create_test_model_with_params("custom-model", 13.0, Some("mistral"), Some("Q8_0"));
        assert_eq!(model.name, "custom-model");
        assert_eq!(model.param_count_b, 13.0);
        assert_eq!(model.architecture, Some("mistral".to_string()));
        assert_eq!(model.quantization, Some("Q8_0".to_string()));
    }

    #[tokio::test]
    async fn test_setup_test_database_with_models() {
        let pool = setup_test_database_with_models().await.unwrap();
        let models = database::list_models(&pool).await.unwrap();
        assert_eq!(models.len(), 4);
    }

    #[test]
    fn test_create_complex_metadata_model() {
        let model = create_complex_metadata_model("complex-test");
        assert_eq!(model.name, "complex-test");
        assert!(model.metadata.len() > 5);
        assert!(model.metadata.contains_key("unicode_test"));
        assert!(model.metadata.contains_key("special_chars"));
    }

    #[test]
    fn test_assert_models_equal_ignore_timestamp() {
        let model1 = create_test_model("test1");
        let mut model2 = create_test_model("test1");

        // Change timestamp
        model2.added_at = Utc::now();

        // Should not panic despite different timestamps
        assert_models_equal_ignore_timestamp(&model1, &model2);
    }
}
