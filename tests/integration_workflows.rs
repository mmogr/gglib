//! Cross-module workflow integration tests.
//!
//! This module tests complete end-to-end workflows that span multiple
//! modules and commands, ensuring the entire system works together.

mod common;

use chrono::Utc;
use common::database::setup_test_pool;
use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

use gglib::commands::{UpdateArgs, update_execute};
use gglib::models::Gguf;
use gglib::services::core::AppCore;
use gglib::services::database;

#[tokio::test]
async fn test_complete_model_lifecycle_workflow() {
    // Test the complete lifecycle: Add -> List -> Update -> Remove
    let pool = setup_test_pool().await.unwrap();
    let temp_dir = tempdir().unwrap();

    // Step 1: Simulate adding a model (like add command would do)
    let file_path = temp_dir.path().join("lifecycle_test.gguf");
    fs::write(&file_path, "dummy gguf content").unwrap();

    let mut initial_metadata = HashMap::new();
    initial_metadata.insert(
        "general.name".to_string(),
        "Lifecycle Test Model".to_string(),
    );
    initial_metadata.insert("general.architecture".to_string(), "llama".to_string());
    initial_metadata.insert("version".to_string(), "1.0".to_string());

    let initial_model = Gguf {
        id: None,
        name: "Lifecycle Test Model".to_string(),
        file_path: file_path.clone(),
        param_count_b: 7.0,
        architecture: Some("llama".to_string()),
        quantization: Some("Q4_0".to_string()),
        context_length: Some(4096),
        metadata: initial_metadata,
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    // Add model (simulating add command)
    database::add_model(&pool, &initial_model).await.unwrap();

    // Step 2: List models (simulating list command)
    let models = database::list_models(&pool).await.unwrap();
    assert_eq!(
        models.len(),
        1,
        "Should have exactly one model after adding"
    );

    let added_model = &models[0];
    let model_id = added_model.id.unwrap();
    assert_eq!(added_model.name, "Lifecycle Test Model");
    assert_eq!(added_model.param_count_b, 7.0);

    // Step 3: Update the model (simulating update command)
    let update_args = UpdateArgs {
        id: model_id,
        name: Some("Updated Lifecycle Model".to_string()),
        param_count: Some(13.0),
        architecture: Some("mistral".to_string()),
        quantization: Some("Q8_0".to_string()),
        context_length: Some(8192),
        metadata: vec!["new_key=new_value".to_string(), "version=2.0".to_string()],
        remove_metadata: None,
        replace_metadata: false, // Merge with existing
        dry_run: false,
        force: true, // Skip confirmation
    };

    update_execute(&AppCore::new(pool.clone()), update_args)
        .await
        .unwrap();

    // Verify update worked
    let updated_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_model.name, "Updated Lifecycle Model");
    assert_eq!(updated_model.param_count_b, 13.0);
    assert_eq!(updated_model.architecture, Some("mistral".to_string()));
    assert_eq!(updated_model.quantization, Some("Q8_0".to_string()));
    assert_eq!(updated_model.context_length, Some(8192));

    // Check metadata merge
    assert!(updated_model.metadata.contains_key("general.name")); // From original
    assert!(updated_model.metadata.contains_key("general.architecture")); // From original
    assert_eq!(
        updated_model.metadata.get("version"),
        Some(&"2.0".to_string())
    ); // Updated
    assert_eq!(
        updated_model.metadata.get("new_key"),
        Some(&"new_value".to_string())
    ); // New

    // Step 4: Remove the model (simulating remove command - find by name, then remove by ID)
    let model_to_remove = database::find_model_by_identifier(&pool, "Updated Lifecycle Model")
        .await
        .unwrap()
        .expect("Model should exist");
    let remove_result = database::remove_model_by_id(&pool, model_to_remove.id.unwrap()).await;
    assert!(remove_result.is_ok(), "Model removal should succeed");

    // Verify removal
    let final_models = database::list_models(&pool).await.unwrap();
    assert_eq!(final_models.len(), 0, "Should have no models after removal");
}

#[tokio::test]
async fn test_multi_model_operations_workflow() {
    // Test operations on multiple models
    let pool = setup_test_pool().await.unwrap();
    let temp_dir = tempdir().unwrap();

    // Add multiple models
    let models_data = vec![
        ("Model Alpha", "llama", 7.0, "Q4_0", 4096),
        ("Model Beta", "mistral", 13.0, "Q8_0", 8192),
        ("Model Gamma", "llama", 30.0, "Q4_0", 4096),
    ];

    for (name, arch, params, quant, ctx) in models_data {
        let file_path = temp_dir
            .path()
            .join(format!("{}.gguf", name.replace(" ", "_").to_lowercase()));
        fs::write(&file_path, format!("dummy content for {}", name)).unwrap();

        let mut metadata = HashMap::new();
        metadata.insert("general.name".to_string(), name.to_string());
        metadata.insert("general.architecture".to_string(), arch.to_string());

        let model = Gguf {
            id: None,
            name: name.to_string(),
            file_path,
            param_count_b: params,
            architecture: Some(arch.to_string()),
            quantization: Some(quant.to_string()),
            context_length: Some(ctx),
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
    }

    // List all models
    let all_models = database::list_models(&pool).await.unwrap();
    assert_eq!(all_models.len(), 3, "Should have three models");

    // Find models by name pattern (simulating search functionality)
    let alpha_models = database::find_models_by_name(&pool, "Alpha").await.unwrap();
    assert_eq!(alpha_models.len(), 1, "Should find one Alpha model");

    let beta_models = database::find_models_by_name(&pool, "Beta").await.unwrap();
    assert_eq!(beta_models.len(), 1, "Should find one Beta model");
    assert_eq!(beta_models[0].name, "Model Beta");

    let gamma_models = database::find_models_by_name(&pool, "Gamma").await.unwrap();
    assert_eq!(gamma_models.len(), 1, "Should find one Gamma model");

    // Update specific model by ID
    let gamma_model = all_models.iter().find(|m| m.name == "Model Gamma").unwrap();
    let update_args = UpdateArgs {
        id: gamma_model.id.unwrap(),
        name: None,                             // Keep existing name
        param_count: None,                      // Keep existing param count
        architecture: None,                     // Keep existing architecture
        quantization: Some("Q2_K".to_string()), // Change quantization
        context_length: Some(2048),             // Change context length
        metadata: vec!["updated=true".to_string()],
        remove_metadata: None,
        replace_metadata: false,
        dry_run: false,
        force: true,
    };

    update_execute(&AppCore::new(pool.clone()), update_args)
        .await
        .unwrap();

    // Verify selective update
    let updated_gamma = database::get_model_by_id(&pool, gamma_model.id.unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_gamma.name, "Model Gamma"); // Unchanged
    assert_eq!(updated_gamma.param_count_b, 30.0); // Unchanged
    assert_eq!(updated_gamma.architecture, Some("llama".to_string())); // Unchanged
    assert_eq!(updated_gamma.quantization, Some("Q2_K".to_string())); // Changed
    assert_eq!(updated_gamma.context_length, Some(2048)); // Changed
    assert_eq!(
        updated_gamma.metadata.get("updated"),
        Some(&"true".to_string())
    ); // Added

    // Remove one model and verify others remain
    let alpha = database::find_model_by_identifier(&pool, "Model Alpha")
        .await
        .unwrap()
        .expect("Model Alpha should exist");
    database::remove_model_by_id(&pool, alpha.id.unwrap())
        .await
        .unwrap();

    let remaining_models = database::list_models(&pool).await.unwrap();
    assert_eq!(
        remaining_models.len(),
        2,
        "Should have two models after removal"
    );

    let remaining_names: Vec<&String> = remaining_models.iter().map(|m| &m.name).collect();
    assert!(remaining_names.contains(&&"Model Beta".to_string()));
    assert!(remaining_names.contains(&&"Model Gamma".to_string()));
    assert!(!remaining_names.contains(&&"Model Alpha".to_string()));
}

#[tokio::test]
async fn test_metadata_manipulation_workflow() {
    // Test complex metadata operations across commands
    let pool = setup_test_pool().await.unwrap();
    let temp_dir = tempdir().unwrap();

    let file_path = temp_dir.path().join("metadata_workflow.gguf");
    fs::write(&file_path, "dummy gguf content").unwrap();

    // Start with rich metadata
    let mut initial_metadata = HashMap::new();
    initial_metadata.insert(
        "general.name".to_string(),
        "Metadata Workflow Model".to_string(),
    );
    initial_metadata.insert("general.architecture".to_string(), "llama".to_string());
    initial_metadata.insert("llama.context_length".to_string(), "4096".to_string());
    initial_metadata.insert("llama.embedding_length".to_string(), "4096".to_string());
    initial_metadata.insert("llama.attention.head_count".to_string(), "32".to_string());
    initial_metadata.insert("tokenizer.ggml.model".to_string(), "llama".to_string());
    initial_metadata.insert("custom.version".to_string(), "1.0".to_string());
    initial_metadata.insert("custom.tag".to_string(), "experimental".to_string());

    let model = Gguf {
        id: None,
        name: "Metadata Workflow Model".to_string(),
        file_path,
        param_count_b: 7.0,
        architecture: Some("llama".to_string()),
        quantization: Some("Q4_0".to_string()),
        context_length: Some(4096),
        metadata: initial_metadata,
        added_at: Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    database::add_model(&pool, &model).await.unwrap();

    let models = database::list_models(&pool).await.unwrap();
    let model_id = models[0].id.unwrap();

    // Test metadata addition and modification
    let update_args = UpdateArgs {
        id: model_id,
        name: None,
        param_count: None,
        architecture: None,
        quantization: None,
        context_length: None,
        metadata: vec![
            "custom.version=2.0".to_string(),            // Update existing
            "custom.status=production".to_string(),      // Add new
            "llama.attention.head_count=64".to_string(), // Update existing
        ],
        remove_metadata: None,
        replace_metadata: false, // Merge
        dry_run: false,
        force: true,
    };

    update_execute(&AppCore::new(pool.clone()), update_args)
        .await
        .unwrap();

    let updated_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        updated_model.metadata.get("custom.version"),
        Some(&"2.0".to_string())
    );
    assert_eq!(
        updated_model.metadata.get("custom.status"),
        Some(&"production".to_string())
    );
    assert_eq!(
        updated_model.metadata.get("llama.attention.head_count"),
        Some(&"64".to_string())
    );
    assert!(updated_model.metadata.contains_key("general.name")); // Original preserved

    // Test metadata removal
    let remove_args = UpdateArgs {
        id: model_id,
        name: None,
        param_count: None,
        architecture: None,
        quantization: None,
        context_length: None,
        metadata: vec![],
        remove_metadata: Some("custom.tag,tokenizer.ggml.model".to_string()),
        replace_metadata: false,
        dry_run: false,
        force: true,
    };

    update_execute(&AppCore::new(pool.clone()), remove_args)
        .await
        .unwrap();

    let final_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();
    assert!(!final_model.metadata.contains_key("custom.tag")); // Removed
    assert!(!final_model.metadata.contains_key("tokenizer.ggml.model")); // Removed
    assert!(final_model.metadata.contains_key("general.name")); // Preserved
    assert!(final_model.metadata.contains_key("custom.version")); // Preserved
    assert!(final_model.metadata.contains_key("custom.status")); // Preserved
}

#[tokio::test]
async fn test_error_handling_across_modules() {
    // Test error handling in various workflow scenarios
    let pool = setup_test_pool().await.unwrap();

    // Test operations on empty database
    let empty_list = database::list_models(&pool).await.unwrap();
    assert_eq!(empty_list.len(), 0);

    let nonexistent_model = database::get_model_by_id(&pool, 999).await.unwrap();
    assert!(nonexistent_model.is_none());

    let search_result = database::find_models_by_name(&pool, "nonexistent")
        .await
        .unwrap();
    assert_eq!(search_result.len(), 0);

    // Test update on nonexistent model (should error)
    let update_args = UpdateArgs {
        id: 999,
        name: Some("Updated Name".to_string()),
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

    let update_result = update_execute(&AppCore::new(pool.clone()), update_args).await;
    // This should error because model with ID 999 doesn't exist
    assert!(update_result.is_err());
    assert!(update_result.unwrap_err().to_string().contains("not found"));

    // Test remove on nonexistent model (should error)
    let remove_result = database::remove_model_by_id(&pool, 999).await;
    assert!(remove_result.is_err());
    assert!(remove_result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_data_consistency_across_operations() {
    // Test that data remains consistent across different operations
    let pool = setup_test_pool().await.unwrap();
    let temp_dir = tempdir().unwrap();

    let file_path = temp_dir.path().join("consistency_test.gguf");
    fs::write(&file_path, "dummy gguf content").unwrap();
    let file_path = fs::canonicalize(&file_path).unwrap_or(file_path);

    let original_time = Utc::now();

    let mut metadata = HashMap::new();
    metadata.insert("consistency.test".to_string(), "true".to_string());

    let model = Gguf {
        id: None,
        name: "Consistency Test Model".to_string(),
        file_path: file_path.clone(),
        param_count_b: 7.0,
        architecture: Some("llama".to_string()),
        quantization: Some("Q4_0".to_string()),
        context_length: Some(4096),
        metadata,
        added_at: original_time,
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: Vec::new(),
    };

    database::add_model(&pool, &model).await.unwrap();

    let models = database::list_models(&pool).await.unwrap();
    let added_model = &models[0];
    let model_id = added_model.id.unwrap();

    // Verify data integrity after add
    assert_eq!(added_model.name, "Consistency Test Model");
    assert_eq!(added_model.file_path, file_path);
    assert_eq!(added_model.param_count_b, 7.0);
    assert!(added_model.id.is_some());
    assert_eq!(
        added_model.metadata.get("consistency.test"),
        Some(&"true".to_string())
    );

    // Update model and verify consistency
    let update_args = UpdateArgs {
        id: model_id,
        name: Some("Updated Consistency Model".to_string()),
        param_count: Some(13.0),
        architecture: None, // Keep existing
        quantization: Some("Q8_0".to_string()),
        context_length: None, // Keep existing
        metadata: vec!["consistency.updated=true".to_string()],
        remove_metadata: None,
        replace_metadata: false,
        dry_run: false,
        force: true,
    };

    update_execute(&AppCore::new(pool.clone()), update_args)
        .await
        .unwrap();

    let updated_model = database::get_model_by_id(&pool, model_id)
        .await
        .unwrap()
        .unwrap();

    // Verify updated fields changed
    assert_eq!(updated_model.name, "Updated Consistency Model");
    assert_eq!(updated_model.param_count_b, 13.0);
    assert_eq!(updated_model.quantization, Some("Q8_0".to_string()));

    // Verify preserved fields unchanged
    assert_eq!(updated_model.id, Some(model_id)); // ID preserved
    assert_eq!(updated_model.file_path, file_path); // File path preserved
    assert_eq!(updated_model.architecture, Some("llama".to_string())); // Architecture preserved
    assert_eq!(updated_model.context_length, Some(4096)); // Context length preserved
    assert_eq!(updated_model.added_at, original_time); // Timestamp preserved

    // Verify metadata consistency
    assert_eq!(
        updated_model.metadata.get("consistency.test"),
        Some(&"true".to_string())
    ); // Original preserved
    assert_eq!(
        updated_model.metadata.get("consistency.updated"),
        Some(&"true".to_string())
    ); // New added
}
