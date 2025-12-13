//! Integration tests for the add command functionality.
//!
//! This module tests the complete add workflow including file validation,
//! metadata extraction, database operations, and error handling.

mod common;

use chrono::Utc;
use common::database::setup_test_pool;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;

use gglib_core::utils::validation;
use gglib_core::{Model, ModelRepository, NewModel};
use gglib_db::SqliteModelRepository;

/// Create a test GGUF file with minimal valid header
fn create_test_gguf_file(temp_dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let file_path = temp_dir.join(format!("{name}.gguf"));
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
        "Error should mention wrong extension, got: {error_msg}"
    );
}

#[tokio::test]
async fn test_add_command_database_integration() {
    let pool = setup_test_pool().await.unwrap();
    let repo: Arc<dyn ModelRepository> = Arc::new(SqliteModelRepository::new(pool));
    let temp_dir = tempdir().unwrap();
    let file_path = create_test_gguf_file(temp_dir.path(), "integration_test");

    // Create a model to add to database
    let mut metadata = HashMap::new();
    metadata.insert(
        "general.name".to_string(),
        "Integration Test Model".to_string(),
    );
    metadata.insert("general.architecture".to_string(), "llama".to_string());

    let mut new_model = NewModel::new(
        "Integration Test Model".to_string(),
        file_path.clone(),
        7.0,
        Utc::now(),
    );
    new_model.architecture = Some("llama".to_string());
    new_model.quantization = Some("Q4_0".to_string());
    new_model.context_length = Some(4096);
    new_model.metadata = metadata;

    // Add model to database
    let result = repo.insert(&new_model).await;
    assert!(
        result.is_ok(),
        "Adding model to database should succeed: {:?}",
        result.err()
    );

    // Verify model was added
    let models = repo.list().await.unwrap();
    assert_eq!(models.len(), 1, "Database should contain exactly one model");

    let added_model: Model = models.into_iter().next().unwrap();
    assert_eq!(added_model.name, "Integration Test Model");
    let expected_path = fs::canonicalize(&file_path).unwrap_or(file_path.clone());
    assert_eq!(added_model.file_path, expected_path);
    assert_eq!(added_model.param_count_b, 7.0);
    assert_eq!(added_model.architecture, Some("llama".to_string()));
    assert!(added_model.id > 0, "Added model should have an ID");
}

#[tokio::test]
async fn test_add_command_duplicate_model_handling() {
    let pool = setup_test_pool().await.unwrap();
    let repo: Arc<dyn ModelRepository> = Arc::new(SqliteModelRepository::new(pool));
    let temp_dir = tempdir().unwrap();
    let file_path = create_test_gguf_file(temp_dir.path(), "duplicate_test");

    // Create first model
    let mut new_model1 = NewModel::new(
        "Duplicate Test".to_string(),
        file_path.clone(),
        7.0,
        Utc::now(),
    );
    new_model1.architecture = Some("llama".to_string());
    new_model1.quantization = Some("Q4_0".to_string());
    new_model1.context_length = Some(4096);

    // Add first model
    let first_model = repo.insert(&new_model1).await.unwrap();
    assert_eq!(first_model.param_count_b, 7.0);
    assert_eq!(first_model.quantization, Some("Q4_0".to_string()));

    // Add second model with same file path (should update via UPSERT for sharded download support)
    let mut new_model2 = NewModel::new(
        "Duplicate Test".to_string(),
        file_path.clone(),
        13.0,
        Utc::now(),
    );
    new_model2.architecture = Some("mistral".to_string());
    new_model2.quantization = Some("Q8_0".to_string());
    new_model2.context_length = Some(8192);

    // This should succeed and update the existing model (UPSERT behavior)
    let updated_model = repo.insert(&new_model2).await.unwrap();

    // Verify it's the same model ID (updated, not inserted)
    assert_eq!(
        updated_model.id, first_model.id,
        "Should update existing model, not create new one"
    );

    // Verify the fields were updated as per UPSERT logic
    assert_eq!(
        updated_model.file_path, first_model.file_path,
        "File path should remain the same"
    );
    assert_eq!(
        updated_model.quantization,
        Some("Q8_0".to_string()),
        "Quantization should be updated"
    );

    // Verify only one model exists in the database
    let models = repo.list().await.unwrap();
    assert_eq!(
        models.len(),
        1,
        "Database should contain only one model (updated via UPSERT)"
    );
}

#[tokio::test]
async fn test_add_command_with_complex_metadata() {
    let pool = setup_test_pool().await.unwrap();
    let repo: Arc<dyn ModelRepository> = Arc::new(SqliteModelRepository::new(pool));
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

    let mut new_model = NewModel::new(
        "Complex Metadata Model".to_string(),
        file_path.clone(),
        7.0,
        Utc::now(),
    );
    new_model.architecture = Some("llama".to_string());
    new_model.quantization = Some("Q4_0".to_string());
    new_model.context_length = Some(4096);
    new_model.metadata = metadata.clone();

    // Add model to database
    repo.insert(&new_model).await.unwrap();

    // Retrieve and verify metadata preservation
    let models = repo.list().await.unwrap();
    let retrieved_model: Model = models.into_iter().next().unwrap();

    assert_eq!(retrieved_model.metadata.len(), metadata.len());
    for (key, value) in &metadata {
        assert_eq!(
            retrieved_model.metadata.get(key),
            Some(value),
            "Metadata key '{key}' should be preserved"
        );
    }
}

#[tokio::test]
async fn test_add_command_with_minimal_data() {
    let pool = setup_test_pool().await.unwrap();
    let repo: Arc<dyn ModelRepository> = Arc::new(SqliteModelRepository::new(pool));
    let temp_dir = tempdir().unwrap();
    let file_path = create_test_gguf_file(temp_dir.path(), "minimal_test");

    // Create model with minimal required data
    let new_model = NewModel::new(
        "Minimal Model".to_string(),
        file_path.clone(),
        1.3,
        Utc::now(),
    );

    // Add model to database
    let result = repo.insert(&new_model).await;
    assert!(result.is_ok(), "Adding minimal model should succeed");

    // Verify model was stored correctly
    let models = repo.list().await.unwrap();
    let retrieved_model: Model = models.into_iter().next().unwrap();

    assert_eq!(retrieved_model.name, "Minimal Model");
    assert_eq!(retrieved_model.param_count_b, 1.3);
    assert_eq!(retrieved_model.architecture, None);
    assert_eq!(retrieved_model.quantization, None);
    assert_eq!(retrieved_model.context_length, None);
    assert!(retrieved_model.metadata.is_empty());
}
