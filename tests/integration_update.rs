//! Integration tests for the update command functionality.
//!
//! This module tests the complete update workflow including database operations,
//! metadata handling, and various update scenarios.

use anyhow::Result;
use chrono::Utc;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

use gglib::commands::{UpdateArgs, update_execute};
use gglib::models::Gguf;
use gglib::services::core::AppCore;
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

/// Create a test database with a sample model
async fn setup_test_database_with_model() -> (sqlx::SqlitePool, Gguf, tempfile::TempDir) {
    let pool = create_test_pool().await.unwrap();

    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), "Test Model".to_string());
    metadata.insert("general.architecture".to_string(), "llama".to_string());
    metadata.insert("custom.tag".to_string(), "test".to_string());

    // Create a temporary file to test file validation
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("test_model.gguf");
    fs::write(&file_path, "dummy gguf content").unwrap();

    let model = Gguf {
        id: None,
        name: "Original Model".to_string(),
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

    database::add_model(&pool, &model).await.unwrap();

    // Get the model back with its ID
    let models = database::list_models(&pool).await.unwrap();
    let added_model = models.into_iter().next().unwrap();

    (pool, added_model, temp_dir)
}

#[tokio::test]
async fn test_update_basic_fields() {
    let (pool, model, _temp_dir) = setup_test_database_with_model().await;
    let model_id = model.id.unwrap();

    let args = UpdateArgs {
        id: model_id,
        name: Some("Updated Model Name".to_string()),
        param_count: Some(13.0),
        architecture: Some("mistral".to_string()),
        quantization: Some("Q8_0".to_string()),
        context_length: Some(8192),
        metadata: vec![],
        remove_metadata: None,
        replace_metadata: false,
        dry_run: false,
        force: true, // Skip confirmation
    };

    update_execute(&AppCore::new(pool.clone()), args).await.unwrap();

    // Verify the updates
    let updated_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_model.name, "Updated Model Name");
    assert_eq!(updated_model.param_count_b, 13.0);
    assert_eq!(updated_model.architecture, Some("mistral".to_string()));
    assert_eq!(updated_model.quantization, Some("Q8_0".to_string()));
    assert_eq!(updated_model.context_length, Some(8192));

    // Original metadata should be preserved
    assert_eq!(updated_model.metadata.len(), 3);
    assert!(updated_model.metadata.contains_key("general.name"));
}

#[tokio::test]
async fn test_update_metadata_merge() {
    let (pool, model, _temp_dir) = setup_test_database_with_model().await;
    let model_id = model.id.unwrap();

    let args = UpdateArgs {
        id: model_id,
        name: None,
        param_count: None,
        architecture: None,
        quantization: None,
        context_length: None,
        metadata: vec![
            "new.key=new.value".to_string(),
            "general.name=Updated via metadata".to_string(),
        ],
        remove_metadata: None,
        replace_metadata: false,
        dry_run: false,
        force: true,
    };

    update_execute(&AppCore::new(pool.clone()), args).await.unwrap();

    let updated_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();

    // Should have original + new metadata
    assert_eq!(updated_model.metadata.len(), 4);
    assert_eq!(
        updated_model.metadata.get("new.key"),
        Some(&"new.value".to_string())
    );
    assert_eq!(
        updated_model.metadata.get("general.name"),
        Some(&"Updated via metadata".to_string())
    );
    assert!(updated_model.metadata.contains_key("general.architecture"));
    assert!(updated_model.metadata.contains_key("custom.tag"));
}

#[tokio::test]
async fn test_update_metadata_replace() {
    let (pool, model, _temp_dir) = setup_test_database_with_model().await;
    let model_id = model.id.unwrap();

    let args = UpdateArgs {
        id: model_id,
        name: None,
        param_count: None,
        architecture: None,
        quantization: None,
        context_length: None,
        metadata: vec!["only.key=only.value".to_string()],
        remove_metadata: None,
        replace_metadata: true,
        dry_run: false,
        force: true,
    };

    update_execute(&AppCore::new(pool.clone()), args).await.unwrap();

    let updated_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();

    // Should only have the new metadata
    assert_eq!(updated_model.metadata.len(), 1);
    assert_eq!(
        updated_model.metadata.get("only.key"),
        Some(&"only.value".to_string())
    );
    assert!(!updated_model.metadata.contains_key("general.name"));
}

#[tokio::test]
async fn test_update_metadata_removal() {
    let (pool, model, _temp_dir) = setup_test_database_with_model().await;
    let model_id = model.id.unwrap();

    let args = UpdateArgs {
        id: model_id,
        name: None,
        param_count: None,
        architecture: None,
        quantization: None,
        context_length: None,
        metadata: vec![],
        remove_metadata: Some("custom.tag,general.architecture".to_string()),
        replace_metadata: false,
        dry_run: false,
        force: true,
    };

    update_execute(&AppCore::new(pool.clone()), args).await.unwrap();

    let updated_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();

    // Should have removed the specified keys - expecting only general.name to remain
    assert_eq!(updated_model.metadata.len(), 1);
    assert!(updated_model.metadata.contains_key("general.name"));
    assert!(!updated_model.metadata.contains_key("custom.tag"));
    assert!(!updated_model.metadata.contains_key("general.architecture"));
}

#[tokio::test]
async fn test_update_nonexistent_model() {
    let (pool, _, _temp_dir) = setup_test_database_with_model().await;

    let args = UpdateArgs {
        id: 999, // Non-existent ID
        name: Some("Should not work".to_string()),
        param_count: None,
        architecture: None,
        quantization: None,
        context_length: None,
        metadata: vec![],
        remove_metadata: None,
        replace_metadata: false,
        dry_run: false,
        force: true,
    };

    let result = update_execute(&AppCore::new(pool.clone()), args).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Model with ID 999 not found")
    );
}

#[tokio::test]
async fn test_update_with_missing_file() {
    let (pool, model, _temp_dir) = setup_test_database_with_model().await;
    let model_id = model.id.unwrap();

    // Remove the file to simulate missing file scenario
    fs::remove_file(&model.file_path).unwrap();

    let args = UpdateArgs {
        id: model_id,
        name: Some("Updated despite missing file".to_string()),
        param_count: None,
        architecture: None,
        quantization: None,
        context_length: None,
        metadata: vec![],
        remove_metadata: None,
        replace_metadata: false,
        dry_run: false,
        force: true, // Force to bypass file check
    };

    // Should succeed with force=true
    update_execute(&AppCore::new(pool.clone()), args).await.unwrap();

    let updated_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_model.name, "Updated despite missing file");
}

#[tokio::test]
async fn test_update_dry_run() {
    let (pool, model, _temp_dir) = setup_test_database_with_model().await;
    let model_id = model.id.unwrap();
    let original_name = model.name.clone();

    let args = UpdateArgs {
        id: model_id,
        name: Some("Should not change".to_string()),
        param_count: Some(999.0),
        architecture: None,
        quantization: None,
        context_length: None,
        metadata: vec!["test.key=test.value".to_string()],
        remove_metadata: None,
        replace_metadata: false,
        dry_run: true, // Dry run mode
        force: true,
    };

    update_execute(&AppCore::new(pool.clone()), args).await.unwrap();

    // Model should be unchanged
    let unchanged_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(unchanged_model.name, original_name);
    assert_eq!(unchanged_model.param_count_b, 7.0);
    assert!(!unchanged_model.metadata.contains_key("test.key"));
}

#[tokio::test]
async fn test_update_complex_metadata_operations() {
    let (pool, model, _temp_dir) = setup_test_database_with_model().await;
    let model_id = model.id.unwrap();

    let args = UpdateArgs {
        id: model_id,
        name: None,
        param_count: None,
        architecture: None,
        quantization: None,
        context_length: None,
        metadata: vec![
            "new.feature=enabled".to_string(),
            "general.name=Complex Updated Name".to_string(),
            "unicode.test=测试 🦙 emoji".to_string(),
        ],
        remove_metadata: Some("custom.tag".to_string()),
        replace_metadata: false,
        dry_run: false,
        force: true,
    };

    update_execute(&AppCore::new(pool.clone()), args).await.unwrap();

    let updated_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();

    // Should have added new keys, updated existing, and removed specified
    assert_eq!(updated_model.metadata.len(), 4);
    assert_eq!(
        updated_model.metadata.get("new.feature"),
        Some(&"enabled".to_string())
    );
    assert_eq!(
        updated_model.metadata.get("general.name"),
        Some(&"Complex Updated Name".to_string())
    );
    assert_eq!(
        updated_model.metadata.get("unicode.test"),
        Some(&"测试 🦙 emoji".to_string())
    );
    assert!(updated_model.metadata.contains_key("general.architecture"));
    assert!(!updated_model.metadata.contains_key("custom.tag"));
}

#[tokio::test]
async fn test_update_partial_fields_only() {
    let (pool, model, _temp_dir) = setup_test_database_with_model().await;
    let model_id = model.id.unwrap();
    let original_param_count = model.param_count_b;
    let original_architecture = model.architecture.clone();

    let args = UpdateArgs {
        id: model_id,
        name: Some("Only name changed".to_string()),
        param_count: None,                      // Keep original
        architecture: None,                     // Keep original
        quantization: Some("Q2_K".to_string()), // Change this
        context_length: None,                   // Keep original
        metadata: vec![],
        remove_metadata: None,
        replace_metadata: false,
        dry_run: false,
        force: true,
    };

    update_execute(&AppCore::new(pool.clone()), args).await.unwrap();

    let updated_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();

    // Only specified fields should change
    assert_eq!(updated_model.name, "Only name changed");
    assert_eq!(updated_model.param_count_b, original_param_count); // Unchanged
    assert_eq!(updated_model.architecture, original_architecture); // Unchanged  
    assert_eq!(updated_model.quantization, Some("Q2_K".to_string())); // Changed
    assert_eq!(updated_model.context_length, Some(4096)); // Unchanged
}
