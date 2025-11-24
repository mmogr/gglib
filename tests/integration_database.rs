//! Integration tests for database operations
//!
//! These tests verify the complete database workflow including:
//! - Database setup and schema creation
//! - Model CRUD operations  
//! - Data integrity and serialization
//! - Error handling scenarios

use chrono::Utc;
use gglib::{models::Gguf, services::database};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;

/// Create a test database with a unique temporary database
async fn create_test_database() -> anyhow::Result<SqlitePool> {
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

/// Create a test model with customizable parameters
fn create_test_model(name: &str, param_count: f64) -> Gguf {
    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), name.to_string());
    metadata.insert("test.version".to_string(), "1.0".to_string());

    Gguf {
        id: None,
        name: name.to_string(),
        file_path: PathBuf::from(format!("/test/models/{}.gguf", name)),
        param_count_b: param_count,
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
async fn test_database_setup() {
    let pool = create_test_database().await;
    assert!(pool.is_ok(), "Database setup should succeed");
}

#[tokio::test]
async fn test_add_and_retrieve_model() {
    let pool = create_test_database().await.unwrap();
    let model = create_test_model("llama-7b-test", 7.0);

    // Add model
    let add_result = database::add_model(&pool, &model).await;
    assert!(add_result.is_ok(), "Adding model should succeed");

    // Retrieve all models
    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(models.len(), 1);

    let retrieved_model = &models[0];
    assert_eq!(retrieved_model.name, "llama-7b-test");
    assert_eq!(retrieved_model.param_count_b, 7.0);
    assert_eq!(retrieved_model.architecture, Some("llama".to_string()));
    assert_eq!(retrieved_model.quantization, Some("Q4_0".to_string()));
    assert_eq!(retrieved_model.context_length, Some(4096));
    assert_eq!(retrieved_model.metadata.len(), 2);
    assert_eq!(
        retrieved_model.metadata.get("general.name"),
        Some(&"llama-7b-test".to_string())
    );
}

#[tokio::test]
async fn test_multiple_models_ordered_by_date() {
    let pool = create_test_database().await.unwrap();

    // Add models with slight delay to ensure different timestamps
    let model1 = create_test_model("model-1", 1.0);
    database::add_model(&pool, &model1).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let model2 = create_test_model("model-2", 2.0);
    database::add_model(&pool, &model2).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let model3 = create_test_model("model-3", 3.0);
    database::add_model(&pool, &model3).await.unwrap();

    // Retrieve models (should be ordered by added_at DESC)
    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(models.len(), 3);
    assert_eq!(models[0].name, "model-3"); // Most recent first
    assert_eq!(models[1].name, "model-2");
    assert_eq!(models[2].name, "model-1"); // Oldest last
}

#[tokio::test]
async fn test_find_models_by_name() {
    let pool = create_test_database().await.unwrap();

    // Add various models
    let models = vec![
        create_test_model("llama-7b-chat", 7.0),
        create_test_model("llama-13b-instruct", 13.0),
        create_test_model("mistral-7b", 7.0),
        create_test_model("codellama-34b", 34.0),
    ];

    for model in &models {
        database::add_model(&pool, model).await.unwrap();
    }

    // Test partial name matching
    let llama_models = database::find_models_by_name(&pool, "llama").await.unwrap();
    assert_eq!(llama_models.len(), 3);

    let mistral_models = database::find_models_by_name(&pool, "mistral")
        .await
        .unwrap();
    assert_eq!(mistral_models.len(), 1);
    assert_eq!(mistral_models[0].name, "mistral-7b");

    let chat_models = database::find_models_by_name(&pool, "chat").await.unwrap();
    assert_eq!(chat_models.len(), 1);
    assert_eq!(chat_models[0].name, "llama-7b-chat");

    let no_match = database::find_models_by_name(&pool, "nonexistent")
        .await
        .unwrap();
    assert_eq!(no_match.len(), 0);
}

#[tokio::test]
async fn test_remove_model() {
    let pool = create_test_database().await.unwrap();
    let model = create_test_model("to-be-removed", 1.0);

    // Add model
    database::add_model(&pool, &model).await.unwrap();
    let models_before = database::list_models(&pool).await.unwrap();
    assert_eq!(models_before.len(), 1);

    // Remove model
    let remove_result = database::remove_model(&pool, "to-be-removed").await;
    assert!(remove_result.is_ok());

    // Verify removal
    let models_after = database::list_models(&pool).await.unwrap();
    assert_eq!(models_after.len(), 0);
}

#[tokio::test]
async fn test_remove_nonexistent_model() {
    let pool = create_test_database().await.unwrap();

    let result = database::remove_model(&pool, "does-not-exist").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No model found"));
}

#[tokio::test]
async fn test_model_with_complex_metadata() {
    let pool = create_test_database().await.unwrap();

    let mut complex_metadata = HashMap::new();
    complex_metadata.insert("general.name".to_string(), "Complex Model".to_string());
    complex_metadata.insert(
        "general.description".to_string(),
        "A model with lots of metadata".to_string(),
    );
    complex_metadata.insert("llama.vocab_size".to_string(), "32000".to_string());
    complex_metadata.insert("llama.attention.head_count".to_string(), "32".to_string());
    complex_metadata.insert(
        "special.characters".to_string(),
        "Test with émojis 🦙 and symbols!".to_string(),
    );

    let model = Gguf {
        id: None,
        name: "complex-metadata-model".to_string(),
        file_path: PathBuf::from("/test/complex.gguf"),
        param_count_b: 7.0,
        architecture: Some("llama".to_string()),
        quantization: Some("Q4_K_M".to_string()),
        context_length: Some(8192),
        metadata: complex_metadata.clone(),
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    // Add and retrieve
    database::add_model(&pool, &model).await.unwrap();
    let retrieved_models = database::list_models(&pool).await.unwrap();

    assert_eq!(retrieved_models.len(), 1);
    let retrieved = &retrieved_models[0];

    // Verify all metadata is preserved
    assert_eq!(retrieved.metadata.len(), 5);
    assert_eq!(
        retrieved.metadata.get("general.name"),
        Some(&"Complex Model".to_string())
    );
    assert_eq!(
        retrieved.metadata.get("special.characters"),
        Some(&"Test with émojis 🦙 and symbols!".to_string())
    );
    assert_eq!(retrieved.quantization, Some("Q4_K_M".to_string()));
    assert_eq!(retrieved.context_length, Some(8192));
}

#[tokio::test]
async fn test_model_with_minimal_fields() {
    let pool = create_test_database().await.unwrap();

    let model = Gguf {
        id: None,
        name: "minimal-model".to_string(),
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
    };

    database::add_model(&pool, &model).await.unwrap();
    let retrieved_models = database::list_models(&pool).await.unwrap();

    assert_eq!(retrieved_models.len(), 1);
    let retrieved = &retrieved_models[0];

    assert_eq!(retrieved.name, "minimal-model");
    assert_eq!(retrieved.param_count_b, 1.3);
    assert_eq!(retrieved.architecture, None);
    assert_eq!(retrieved.quantization, None);
    assert_eq!(retrieved.context_length, None);
    assert!(retrieved.metadata.is_empty());
}

#[tokio::test]
async fn test_concurrent_database_operations() {
    let pool = create_test_database().await.unwrap();

    // Spawn multiple concurrent tasks
    let mut handles = vec![];

    for i in 0..10 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            let model = create_test_model(&format!("concurrent-model-{}", i), i as f64);
            database::add_model(&pool_clone, &model).await
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(
            result.is_ok(),
            "Concurrent database operation should succeed"
        );
    }

    // Verify all models were added
    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(models.len(), 10);
}
