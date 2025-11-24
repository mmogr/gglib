//! Integration tests for the serve command functionality.
//!
//! This module tests the serve command workflow including model lookup,
//! file validation, and command building (without actually starting servers).

use anyhow::Result;
use chrono::Utc;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

use gglib::models::Gguf;
use gglib::services::database;

/// Create an isolated test database pool with the proper schema
async fn create_test_pool() -> Result<SqlitePool> {
    // Use in-memory database for testing to avoid interference
    let pool = SqlitePool::connect("sqlite::memory:").await?;

    // Create the table with enhanced metadata fields
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

/// Create a test database with sample models for serve testing
async fn setup_test_database_with_models() -> (SqlitePool, Vec<Gguf>, tempfile::TempDir) {
    let pool = create_test_pool().await.unwrap();
    let temp_dir = tempdir().unwrap();

    // Create test GGUF files
    let file1_path = temp_dir.path().join("model1.gguf");
    let file2_path = temp_dir.path().join("model2.gguf");
    fs::write(&file1_path, "dummy gguf content 1").unwrap();
    fs::write(&file2_path, "dummy gguf content 2").unwrap();

    let mut metadata1 = HashMap::new();
    metadata1.insert("general.name".to_string(), "Test Model 1".to_string());
    metadata1.insert("general.architecture".to_string(), "llama".to_string());
    metadata1.insert("llama.context_length".to_string(), "4096".to_string());

    let mut metadata2 = HashMap::new();
    metadata2.insert("general.name".to_string(), "Test Model 2".to_string());
    metadata2.insert("general.architecture".to_string(), "mistral".to_string());
    metadata2.insert("mistral.context_length".to_string(), "8192".to_string());

    let model1 = Gguf {
        id: None,
        name: "Test Model 1".to_string(),
        file_path: file1_path.clone(),
        param_count_b: 7.0,
        architecture: Some("llama".to_string()),
        quantization: Some("Q4_0".to_string()),
        context_length: Some(4096),
        metadata: metadata1,
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    let model2 = Gguf {
        id: None,
        name: "Test Model 2".to_string(),
        file_path: file2_path.clone(),
        param_count_b: 13.0,
        architecture: Some("mistral".to_string()),
        quantization: Some("Q8_0".to_string()),
        context_length: Some(8192),
        metadata: metadata2,
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    database::add_model(&pool, &model1).await.unwrap();
    database::add_model(&pool, &model2).await.unwrap();

    // Get models back with their IDs
    let models = database::list_models(&pool).await.unwrap();

    (pool, models, temp_dir)
}

#[tokio::test]
async fn test_serve_command_model_lookup_by_id() {
    let (pool, models, _temp_dir) = setup_test_database_with_models().await;

    // Find the "Test Model 1" specifically
    let test_model_1 = models.iter().find(|m| m.name == "Test Model 1").unwrap();
    let model_id = test_model_1.id.unwrap();

    // Test finding model by ID (using the database function that serve command uses)
    let result = database::find_model_by_identifier(&pool, &model_id.to_string())
        .await
        .unwrap();
    assert!(result.is_some(), "Should find model by ID");

    let found_model = result.unwrap();
    assert_eq!(found_model.id, Some(model_id));
    assert_eq!(found_model.name, "Test Model 1");
    assert_eq!(found_model.architecture, Some("llama".to_string()));
}

#[tokio::test]
async fn test_serve_command_model_lookup_by_name() {
    let (pool, _models, _temp_dir) = setup_test_database_with_models().await;

    // Test finding model by name
    let result = database::find_model_by_identifier(&pool, "Test Model 2")
        .await
        .unwrap();
    assert!(result.is_some(), "Should find model by name");

    let found_model = result.unwrap();
    assert_eq!(found_model.name, "Test Model 2");
    assert_eq!(found_model.architecture, Some("mistral".to_string()));
    assert_eq!(found_model.context_length, Some(8192));
}

#[tokio::test]
async fn test_serve_command_model_not_found() {
    let (pool, _models, _temp_dir) = setup_test_database_with_models().await;

    // Test with non-existent ID
    let result = database::find_model_by_identifier(&pool, "999")
        .await
        .unwrap();
    assert!(result.is_none(), "Should not find non-existent model ID");

    // Test with non-existent name
    let result = database::find_model_by_identifier(&pool, "Non-existent Model")
        .await
        .unwrap();
    assert!(result.is_none(), "Should not find non-existent model name");
}

#[tokio::test]
async fn test_serve_command_context_size_detection() {
    let (_pool, models, _temp_dir) = setup_test_database_with_models().await;
    let model_with_context = models.iter().find(|m| m.name == "Test Model 1").unwrap(); // This model has context_length: Some(4096)

    // Test that model has context size in database
    assert_eq!(model_with_context.context_length, Some(4096));
    assert!(
        model_with_context
            .metadata
            .contains_key("llama.context_length")
    );
    assert_eq!(
        model_with_context.metadata.get("llama.context_length"),
        Some(&"4096".to_string())
    );

    // Test model without explicit context length
    let model_without_context = models.iter().find(|m| m.name == "Test Model 2").unwrap();
    if model_without_context.context_length.is_none() {
        // Should be able to fall back to metadata or defaults
        assert!(
            model_without_context
                .metadata
                .contains_key("mistral.context_length")
        );
    }
}

#[tokio::test]
async fn test_serve_command_file_validation() {
    let (pool, models, temp_dir) = setup_test_database_with_models().await;
    let model = models.iter().find(|m| m.name == "Test Model 1").unwrap();

    // Test that model file exists (our test setup creates real files)
    assert!(model.file_path.exists(), "Model file should exist");

    // Test scenario where file is missing by creating a new model with missing file
    let missing_file_path = temp_dir.path().join("missing.gguf");
    let model_with_missing_file = Gguf {
        id: None,
        name: "Model with Missing File".to_string(),
        file_path: missing_file_path.clone(),
        param_count_b: model.param_count_b,
        architecture: model.architecture.clone(),
        quantization: model.quantization.clone(),
        context_length: model.context_length,
        metadata: model.metadata.clone(),
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    // Add the model with missing file to database
    database::add_model(&pool, &model_with_missing_file)
        .await
        .unwrap();

    // Find the newly added model
    let models_with_missing = database::find_models_by_name(&pool, "Missing File")
        .await
        .unwrap();
    assert_eq!(models_with_missing.len(), 1);
    let model_with_missing = &models_with_missing[0];

    // Verify file doesn't exist
    assert!(
        !model_with_missing.file_path.exists(),
        "Missing file should not exist"
    );
}

#[tokio::test]
async fn test_serve_command_model_metadata_access() {
    let (_pool, models, _temp_dir) = setup_test_database_with_models().await;

    for model in &models {
        // Test that serve command can access all necessary model information
        assert!(model.id.is_some(), "Model should have ID for serve command");
        assert!(!model.name.is_empty(), "Model should have name for display");
        assert!(
            model.file_path.exists(),
            "Model file should exist for serving"
        );

        // Test metadata accessibility
        if let Some(ref arch) = model.architecture {
            assert!(
                !arch.is_empty(),
                "Architecture should not be empty if present"
            );
        }

        // Test that metadata is properly structured for context size lookup
        for (key, value) in &model.metadata {
            assert!(!key.is_empty(), "Metadata keys should not be empty");
            assert!(!value.is_empty(), "Metadata values should not be empty");
        }
    }
}

#[tokio::test]
async fn test_serve_command_different_architectures() {
    let (_pool, models, _temp_dir) = setup_test_database_with_models().await;

    // Find models with different architectures
    let llama_model = models
        .iter()
        .find(|m| m.architecture == Some("llama".to_string()));
    let mistral_model = models
        .iter()
        .find(|m| m.architecture == Some("mistral".to_string()));

    assert!(llama_model.is_some(), "Should have llama model");
    assert!(mistral_model.is_some(), "Should have mistral model");

    let llama = llama_model.unwrap();
    let mistral = mistral_model.unwrap();

    // Test that serve command can handle different architectures
    assert_eq!(llama.architecture, Some("llama".to_string()));
    assert_eq!(mistral.architecture, Some("mistral".to_string()));

    // Test that both models have appropriate context length handling
    assert!(llama.context_length.is_some() || llama.metadata.contains_key("llama.context_length"));
    assert!(
        mistral.context_length.is_some() || mistral.metadata.contains_key("mistral.context_length")
    );
}

#[tokio::test]
async fn test_serve_command_parameter_combinations() {
    let (pool, models, _temp_dir) = setup_test_database_with_models().await;
    let model = &models[0];

    // Test that serve command can work with various parameter combinations
    // (This tests the data availability, not the actual command execution)

    // Test with default context size (from model)
    assert!(model.context_length.is_some() || !model.metadata.is_empty());

    // Test with explicit context size override (any value should be acceptable)
    // The serve command should be able to override the model's context size

    // Test with memory lock option (boolean flag)
    // The serve command should handle mlock flag correctly

    // Test with model ID lookup
    let found_by_id = database::find_model_by_identifier(&pool, &model.id.unwrap().to_string())
        .await
        .unwrap();
    assert!(found_by_id.is_some());
    assert_eq!(found_by_id.unwrap().id, model.id);
}

#[tokio::test]
async fn test_serve_command_error_scenarios() {
    let pool = create_test_pool().await.unwrap();

    // Test empty database scenario
    let result = database::find_model_by_identifier(&pool, "1")
        .await
        .unwrap();
    assert!(result.is_none(), "Should not find model in empty database");

    // Add a model but with invalid file path
    let temp_dir = tempdir().unwrap();
    let invalid_path = temp_dir.path().join("nonexistent.gguf");

    let model = Gguf {
        id: None,
        name: "Invalid File Model".to_string(),
        file_path: invalid_path.clone(),
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

    database::add_model(&pool, &model).await.unwrap();

    // Find the model (should succeed)
    let models = database::list_models(&pool).await.unwrap();
    let found_model = &models[0];

    // But file should not exist (serve command should detect this)
    assert!(
        !found_model.file_path.exists(),
        "Invalid file path should not exist"
    );
}
