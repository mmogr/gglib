//! Integration tests for command handlers
//!
//! These tests verify the end-to-end functionality of CLI commands
//! including database interactions, user workflows, and error handling.

use chrono::Utc;
use gglib::{
    models::Gguf,
    services::database::{self, ModelStoreError},
};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;

/// Create a test database pool for command testing
async fn create_test_database() -> anyhow::Result<SqlitePool> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;

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

    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_models_file_path ON models(file_path)")
        .execute(&pool)
        .await?;

    Ok(pool)
}

/// Create a test model for command testing
fn create_test_model(name: &str) -> Gguf {
    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), name.to_string());

    Gguf {
        id: None,
        name: name.to_string(),
        file_path: PathBuf::from(format!("/test/{}.gguf", name)),
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
    }
}

#[tokio::test]
async fn test_list_command_with_no_models() {
    // This test would require mocking database::setup_database() to return our test pool
    // For now, we test the command logic indirectly through the database functions

    let pool = create_test_database().await.unwrap();
    let models = database::list_models(&pool).await.unwrap();

    // Empty database should return empty list
    assert_eq!(models.len(), 0);
}

#[tokio::test]
async fn test_list_command_with_models() {
    let pool = create_test_database().await.unwrap();

    // Add some test models
    let model1 = create_test_model("test-model-1");
    let model2 = create_test_model("test-model-2");

    database::add_model(&pool, &model1).await.unwrap();
    database::add_model(&pool, &model2).await.unwrap();

    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(models.len(), 2);
}

#[tokio::test]
async fn test_remove_command_scenarios() {
    let pool = create_test_database().await.unwrap();

    // Test removing non-existent model by ID
    let result = database::remove_model_by_id(&pool, 999).await;
    assert!(result.is_err());

    // Add a model and then remove it
    let model = create_test_model("to-remove");
    database::add_model(&pool, &model).await.unwrap();

    let models_before = database::list_models(&pool).await.unwrap();
    assert_eq!(models_before.len(), 1);
    let model_id = models_before[0].id.unwrap();

    let remove_result = database::remove_model_by_id(&pool, model_id).await;
    assert!(remove_result.is_ok());

    let models_after = database::list_models(&pool).await.unwrap();
    assert_eq!(models_after.len(), 0);
}

#[tokio::test]
async fn test_find_models_for_remove_command() {
    let pool = create_test_database().await.unwrap();

    // Add models with similar names
    let models = vec![
        create_test_model("llama-7b-chat"),
        create_test_model("llama-7b-instruct"),
        create_test_model("llama-13b-chat"),
    ];

    for model in &models {
        database::add_model(&pool, model).await.unwrap();
    }

    // Test partial matching (what remove command would use)
    let chat_models = database::find_models_by_name(&pool, "chat").await.unwrap();
    assert_eq!(chat_models.len(), 2);

    let exact_match = database::find_models_by_name(&pool, "llama-7b-chat")
        .await
        .unwrap();
    assert_eq!(exact_match.len(), 1);
    assert_eq!(exact_match[0].name, "llama-7b-chat");

    let no_match = database::find_models_by_name(&pool, "nonexistent")
        .await
        .unwrap();
    assert_eq!(no_match.len(), 0);
}

// Note: Testing the actual command handlers (handle_add, handle_list, handle_remove)
// would require either:
// 1. Dependency injection to pass in test database pools
// 2. Mocking the database module
// 3. Environment variables or configuration to use test databases
// 4. Refactoring commands to accept database pools as parameters

// For now, we test the core business logic through the database layer

#[tokio::test]
async fn test_model_lifecycle_workflow() {
    let pool = create_test_database().await.unwrap();

    // Simulate the full workflow: add -> list -> find -> remove

    // 1. Start with empty database
    let initial_models = database::list_models(&pool).await.unwrap();
    assert_eq!(initial_models.len(), 0);

    // 2. Add a model (simulating add command)
    let model = create_test_model("lifecycle-test-model");
    database::add_model(&pool, &model).await.unwrap();

    // 3. List models (simulating list command)
    let after_add = database::list_models(&pool).await.unwrap();
    assert_eq!(after_add.len(), 1);
    assert_eq!(after_add[0].name, "lifecycle-test-model");

    // 4. Find the model (simulating remove command search)
    let found_models = database::find_models_by_name(&pool, "lifecycle-test")
        .await
        .unwrap();
    assert_eq!(found_models.len(), 1);

    // 5. Remove the model (simulating remove command - find by name, remove by ID)
    let model_to_remove = database::find_model_by_identifier(&pool, "lifecycle-test-model")
        .await
        .unwrap()
        .expect("Model should exist");
    let remove_result = database::remove_model_by_id(&pool, model_to_remove.id.unwrap()).await;
    assert!(remove_result.is_ok());

    // 6. Verify removal
    let final_models = database::list_models(&pool).await.unwrap();
    assert_eq!(final_models.len(), 0);
}

#[tokio::test]
async fn test_command_error_scenarios() {
    let pool = create_test_database().await.unwrap();

    // Test error scenarios that commands need to handle

    // 1. Remove non-existent model by ID
    let remove_error = database::remove_model_by_id(&pool, 999).await;
    assert!(remove_error.is_err());
    assert!(
        remove_error
            .unwrap_err()
            .to_string()
            .contains("not found")
    );

    // 2. Search for non-existent model
    let search_result = database::find_models_by_name(&pool, "non-existent")
        .await
        .unwrap();
    assert_eq!(search_result.len(), 0);

    // 3. Add model with duplicate name (should now be rejected)
    let model1 = create_test_model("duplicate-name");
    let model2 = create_test_model("duplicate-name");

    database::add_model(&pool, &model1).await.unwrap();
    let duplicate_error = database::add_model(&pool, &model2)
        .await
        .expect_err("Duplicate entries should now be rejected");

    let store_error = duplicate_error
        .downcast::<ModelStoreError>()
        .expect("Error should be a ModelStoreError");

    match store_error {
        ModelStoreError::DuplicateModel { model_name, .. } => {
            assert_eq!(model_name, "duplicate-name");
        }
        other => panic!("Expected DuplicateModel error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_models_with_various_metadata() {
    let pool = create_test_database().await.unwrap();

    // Test models with different metadata configurations
    let models = vec![
        // Full metadata
        Gguf {
            id: None,
            name: "full-metadata".to_string(),
            file_path: PathBuf::from("/test/full.gguf"),
            param_count_b: 7.0,
            architecture: Some("llama".to_string()),
            quantization: Some("Q4_0".to_string()),
            context_length: Some(4096),
            metadata: {
                let mut m = HashMap::new();
                m.insert("general.name".to_string(), "Full Model".to_string());
                m.insert(
                    "general.description".to_string(),
                    "A model with all fields".to_string(),
                );
                m
            },
            added_at: Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: Vec::new(),
        },
        // Minimal metadata
        Gguf {
            id: None,
            name: "minimal-metadata".to_string(),
            file_path: PathBuf::from("/test/minimal.gguf"),
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
        },
    ];

    // Add all models
    for model in &models {
        database::add_model(&pool, model).await.unwrap();
    }

    // Retrieve and verify
    let retrieved_models = database::list_models(&pool).await.unwrap();
    assert_eq!(retrieved_models.len(), 2);

    // Find each model and verify its metadata
    let full_model = database::find_models_by_name(&pool, "full-metadata")
        .await
        .unwrap();
    assert_eq!(full_model.len(), 1);
    assert_eq!(full_model[0].architecture, Some("llama".to_string()));
    assert_eq!(full_model[0].metadata.len(), 2);

    let minimal_model = database::find_models_by_name(&pool, "minimal-metadata")
        .await
        .unwrap();
    assert_eq!(minimal_model.len(), 1);
    assert_eq!(minimal_model[0].architecture, None);
    assert!(minimal_model[0].metadata.is_empty());
}

// Additional tests you might want to add:
// - Test command line argument parsing
// - Test user input validation
// - Test file path handling and validation
// - Test concurrent command execution
// - Test command output formatting
// - Test error message formatting and user experience
