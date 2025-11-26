//! Integration tests for the add command functionality.
//!
//! This module tests the complete add workflow including file validation,
//! metadata extraction, database operations, and error handling.

mod common;

use chrono::Utc;
use common::database::setup_test_pool;
use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

use gglib::models::Gguf;
use gglib::services::database::{self, ModelStoreError};
use gglib::utils::validation;

/// Create a test GGUF file with minimal valid header
fn create_test_gguf_file(temp_dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let file_path = temp_dir.join(format!("{}.gguf", name));
    // Create a minimal GGUF file with correct header
    // GGUF magic number is "GGUF" (0x46554747)
    let gguf_header = [
        0x47, 0x47, 0x55, 0x46, // Magic "GGUF"
        0x03, 0x00, 0x00, 0x00, // Version 3
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Tensor count (1)
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Metadata count (1)
    ];
    fs::write(&file_path, gguf_header).unwrap();
    file_path
}

#[tokio::test]
async fn test_add_command_file_validation_success() {
    let temp_dir = tempdir().unwrap();
    let file_path = create_test_gguf_file(temp_dir.path(), "test_model");

    // Test that file validation passes for valid GGUF file
    let result = validation::validate_file(file_path.to_str().unwrap());
    assert!(
        result.is_ok(),
        "File validation should succeed for valid GGUF file"
    );
}

#[tokio::test]
async fn test_add_command_file_validation_failure() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("invalid.txt");
    fs::write(&file_path, "not a gguf file").unwrap();

    // Test that file validation fails for non-GGUF file
    let result = validation::validate_file(file_path.to_str().unwrap());
    assert!(
        result.is_err(),
        "File validation should fail for non-GGUF file"
    );
}

#[tokio::test]
async fn test_add_command_nonexistent_file() {
    let result = validation::validate_file("/path/that/does/not/exist.gguf");
    assert!(
        result.is_err(),
        "File validation should fail for nonexistent file"
    );

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("File does not exist"),
        "Error should mention file doesn't exist"
    );
}

#[tokio::test]
async fn test_add_command_wrong_extension() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("model.bin");
    fs::write(&file_path, "some content").unwrap();

    let result = validation::validate_file(file_path.to_str().unwrap());
    assert!(
        result.is_err(),
        "File validation should fail for wrong extension"
    );

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("Wrong extension"),
        "Error should mention wrong extension, got: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_add_command_database_integration() {
    let pool = setup_test_pool().await.unwrap();
    let temp_dir = tempdir().unwrap();
    let file_path = create_test_gguf_file(temp_dir.path(), "integration_test");

    // Create a model to add to database
    let mut metadata = HashMap::new();
    metadata.insert(
        "general.name".to_string(),
        "Integration Test Model".to_string(),
    );
    metadata.insert("general.architecture".to_string(), "llama".to_string());

    let model = Gguf {
        id: None,
        name: "Integration Test Model".to_string(),
        file_path: file_path.clone(),
        param_count_b: 7.0,
        architecture: Some("llama".to_string()),
        quantization: Some("Q4_0".to_string()),
        context_length: Some(4096),
        metadata,
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    // Add model to database
    let result = database::add_model(&pool, &model).await;
    assert!(result.is_ok(), "Adding model to database should succeed");

    // Verify model was added
    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(models.len(), 1, "Database should contain exactly one model");

    let added_model = &models[0];
    assert_eq!(added_model.name, "Integration Test Model");
    let expected_path = fs::canonicalize(&file_path).unwrap_or(file_path.clone());
    assert_eq!(added_model.file_path, expected_path);
    assert_eq!(added_model.param_count_b, 7.0);
    assert_eq!(added_model.architecture, Some("llama".to_string()));
    assert!(added_model.id.is_some(), "Added model should have an ID");
}

#[tokio::test]
async fn test_add_command_duplicate_model_handling() {
    let pool = setup_test_pool().await.unwrap();
    let temp_dir = tempdir().unwrap();
    let file_path = create_test_gguf_file(temp_dir.path(), "duplicate_test");

    // Create first model
    let model1 = Gguf {
        id: None,
        name: "Duplicate Test".to_string(),
        file_path: file_path.clone(),
        param_count_b: 7.0,
        architecture: Some("llama".to_string()),
        quantization: Some("Q4_0".to_string()),
        context_length: Some(4096),
        metadata: HashMap::new(),
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    // Add first model
    database::add_model(&pool, &model1).await.unwrap();

    // Try to add second model with same name (should succeed - duplicates allowed)
    let model2 = Gguf {
        id: None,
        name: "Duplicate Test".to_string(),
        file_path: file_path.clone(),
        param_count_b: 13.0,
        architecture: Some("mistral".to_string()),
        quantization: Some("Q8_0".to_string()),
        context_length: Some(8192),
        metadata: HashMap::new(),
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    let err = database::add_model(&pool, &model2)
        .await
        .expect_err("Duplicate file path should now be rejected");

    let store_err = err
        .downcast::<ModelStoreError>()
        .expect("Error should be ModelStoreError");

    match store_err {
        ModelStoreError::DuplicateModel { file_path, .. } => {
            assert!(file_path.contains("duplicate_test"));
        }
        other => panic!("Expected DuplicateModel error, got {:?}", other),
    }

    // Verify only the first model exists
    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(
        models.len(),
        1,
        "Database should contain only the original model"
    );
}

#[tokio::test]
async fn test_add_command_with_complex_metadata() {
    let pool = setup_test_pool().await.unwrap();
    let temp_dir = tempdir().unwrap();
    let file_path = create_test_gguf_file(temp_dir.path(), "metadata_test");

    // Create model with complex metadata
    let mut metadata = HashMap::new();
    metadata.insert(
        "general.name".to_string(),
        "Complex Metadata Model".to_string(),
    );
    metadata.insert("general.architecture".to_string(), "llama".to_string());
    metadata.insert("llama.context_length".to_string(), "4096".to_string());
    metadata.insert("llama.embedding_length".to_string(), "4096".to_string());
    metadata.insert("llama.feed_forward_length".to_string(), "11008".to_string());
    metadata.insert("llama.attention.head_count".to_string(), "32".to_string());
    metadata.insert(
        "llama.attention.head_count_kv".to_string(),
        "32".to_string(),
    );
    metadata.insert("tokenizer.ggml.model".to_string(), "llama".to_string());
    metadata.insert("custom.tag".to_string(), "production".to_string());
    metadata.insert("custom.version".to_string(), "2.1.0".to_string());

    let model = Gguf {
        id: None,
        name: "Complex Metadata Model".to_string(),
        file_path: file_path.clone(),
        param_count_b: 7.0,
        architecture: Some("llama".to_string()),
        quantization: Some("Q4_0".to_string()),
        context_length: Some(4096),
        metadata: metadata.clone(),
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    // Add model to database
    database::add_model(&pool, &model).await.unwrap();

    // Retrieve and verify metadata preservation
    let models = database::list_models(&pool).await.unwrap();
    let retrieved_model = &models[0];

    assert_eq!(retrieved_model.metadata.len(), metadata.len());
    for (key, value) in &metadata {
        assert_eq!(
            retrieved_model.metadata.get(key),
            Some(value),
            "Metadata key '{}' should be preserved",
            key
        );
    }
}

#[tokio::test]
async fn test_add_command_with_minimal_data() {
    let pool = setup_test_pool().await.unwrap();
    let temp_dir = tempdir().unwrap();
    let file_path = create_test_gguf_file(temp_dir.path(), "minimal_test");

    // Create model with minimal required data
    let model = Gguf {
        id: None,
        name: "Minimal Model".to_string(),
        file_path: file_path.clone(),
        param_count_b: 1.3,
        architecture: None,
        quantization: None,
        context_length: None,
        metadata: HashMap::new(),
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    // Add model to database
    let result = database::add_model(&pool, &model).await;
    assert!(result.is_ok(), "Adding minimal model should succeed");

    // Verify model was stored correctly
    let models = database::list_models(&pool).await.unwrap();
    let retrieved_model = &models[0];

    assert_eq!(retrieved_model.name, "Minimal Model");
    assert_eq!(retrieved_model.param_count_b, 1.3);
    assert_eq!(retrieved_model.architecture, None);
    assert_eq!(retrieved_model.quantization, None);
    assert_eq!(retrieved_model.context_length, None);
    assert!(retrieved_model.metadata.is_empty());
}
