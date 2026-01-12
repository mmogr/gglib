//! Integration tests for the question command functionality.
//!
//! This module tests the question command workflow including:
//! - Default model configuration via settings
//! - Model resolution fallback chain
//! - Settings persistence

mod common;

use common::database::setup_test_pool;
use std::sync::Arc;

use gglib_core::{
    ModelRepository, NewModel, Settings, SettingsUpdate,
    services::SettingsService,
};
use gglib_db::{SqliteModelRepository, SqliteSettingsRepository};
use chrono::Utc;
use tempfile::tempdir;
use std::fs;

/// Create a test GGUF file with minimal valid header
fn create_test_gguf_file(temp_dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let file_path = temp_dir.join(format!("{name}.gguf"));
    // Create a minimal GGUF file with correct header
    let gguf_header = [
        0x47, 0x47, 0x55, 0x46, // Magic "GGUF"
        0x03, 0x00, 0x00, 0x00, // Version 3
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Tensor count (1)
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Metadata count (1)
    ];
    fs::write(&file_path, gguf_header).unwrap();
    file_path
}

// =============================================================================
// Settings: default_model_id persistence tests
// =============================================================================

#[tokio::test]
async fn test_settings_default_model_id_initially_none() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteSettingsRepository::new(pool);
    let service = SettingsService::new(Arc::new(repo));

    let settings = service.get().await.unwrap();
    assert_eq!(
        settings.default_model_id, None,
        "default_model_id should be None initially"
    );
}

#[tokio::test]
async fn test_settings_set_and_get_default_model_id() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteSettingsRepository::new(pool);
    let service = SettingsService::new(Arc::new(repo));

    // Set default model ID
    let update = SettingsUpdate {
        default_model_id: Some(Some(42)),
        ..Default::default()
    };
    service.update(update).await.unwrap();

    // Retrieve and verify
    let settings = service.get().await.unwrap();
    assert_eq!(
        settings.default_model_id,
        Some(42),
        "default_model_id should be 42 after update"
    );
}

#[tokio::test]
async fn test_settings_clear_default_model_id() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteSettingsRepository::new(pool);
    let service = SettingsService::new(Arc::new(repo));

    // First set a value
    let update = SettingsUpdate {
        default_model_id: Some(Some(42)),
        ..Default::default()
    };
    service.update(update).await.unwrap();

    // Verify it's set
    let settings = service.get().await.unwrap();
    assert_eq!(settings.default_model_id, Some(42));

    // Now clear it
    let clear_update = SettingsUpdate {
        default_model_id: Some(None),
        ..Default::default()
    };
    service.update(clear_update).await.unwrap();

    // Verify it's cleared
    let settings = service.get().await.unwrap();
    assert_eq!(
        settings.default_model_id, None,
        "default_model_id should be None after clearing"
    );
}

#[tokio::test]
async fn test_settings_default_model_id_survives_other_updates() {
    let pool = setup_test_pool().await.unwrap();
    let repo = SqliteSettingsRepository::new(pool);
    let service = SettingsService::new(Arc::new(repo));

    // Set default model ID
    let update = SettingsUpdate {
        default_model_id: Some(Some(42)),
        ..Default::default()
    };
    service.update(update).await.unwrap();

    // Update a different setting
    let other_update = SettingsUpdate {
        proxy_port: Some(Some(9999)),
        ..Default::default()
    };
    service.update(other_update).await.unwrap();

    // Verify default_model_id is preserved
    let settings = service.get().await.unwrap();
    assert_eq!(
        settings.default_model_id,
        Some(42),
        "default_model_id should survive other updates"
    );
    assert_eq!(
        settings.proxy_port,
        Some(9999),
        "proxy_port should be updated"
    );
}

// =============================================================================
// Model resolution tests
// =============================================================================

#[tokio::test]
async fn test_find_model_by_id() {
    let pool = setup_test_pool().await.unwrap();
    let model_repo = SqliteModelRepository::new(pool);
    let temp_dir = tempdir().unwrap();
    let file_path = create_test_gguf_file(temp_dir.path(), "test_model");

    // Add a model
    let new_model = NewModel::new(
        "Test Model".to_string(),
        file_path.clone(),
        7.0,
        Utc::now(),
    );
    let inserted = model_repo.insert(&new_model).await.unwrap();
    let id = inserted.id;

    // Find by ID - returns Result<Model>
    let found = model_repo.get_by_id(id).await;
    assert!(found.is_ok(), "Model should be found by ID");
    let model = found.unwrap();
    assert_eq!(model.id, id);
    assert_eq!(model.name, "Test Model");
}

#[tokio::test]
async fn test_find_model_by_name() {
    let pool = setup_test_pool().await.unwrap();
    let model_repo = SqliteModelRepository::new(pool);
    let temp_dir = tempdir().unwrap();
    let file_path = create_test_gguf_file(temp_dir.path(), "named_model");

    // Add a model
    let new_model = NewModel::new(
        "Named Model".to_string(),
        file_path.clone(),
        7.0,
        Utc::now(),
    );
    model_repo.insert(&new_model).await.unwrap();

    // Find by name using get_by_name
    let found = model_repo.get_by_name("Named Model").await;
    assert!(found.is_ok(), "Model should be found by name");
    let model = found.unwrap();
    assert_eq!(model.name, "Named Model");
}

#[tokio::test]
async fn test_find_model_not_found() {
    let pool = setup_test_pool().await.unwrap();
    let model_repo = SqliteModelRepository::new(pool);

    // Try to find a model that doesn't exist
    let found = model_repo.get_by_name("nonexistent model").await;
    assert!(found.is_err(), "Should error when model not found");
}

#[tokio::test]
async fn test_find_model_by_id_not_found() {
    let pool = setup_test_pool().await.unwrap();
    let model_repo = SqliteModelRepository::new(pool);

    // Try to find a model by ID that doesn't exist
    let found = model_repo.get_by_id(99999).await;
    assert!(found.is_err(), "Should error when model ID not found");
}

// =============================================================================
// Settings merge behavior tests
// =============================================================================

#[test]
fn test_settings_merge_preserves_unset_fields() {
    let mut settings = Settings::with_defaults();
    settings.default_model_id = Some(42);

    let update = SettingsUpdate {
        proxy_port: Some(Some(9999)),
        ..Default::default()
    };

    settings.merge(&update);

    assert_eq!(
        settings.default_model_id,
        Some(42),
        "Merge should preserve default_model_id when not in update"
    );
    assert_eq!(
        settings.proxy_port,
        Some(9999),
        "Merge should apply proxy_port from update"
    );
}

#[test]
fn test_settings_merge_updates_default_model_id() {
    let mut settings = Settings::with_defaults();
    settings.default_model_id = Some(42);

    let update = SettingsUpdate {
        default_model_id: Some(Some(99)),
        ..Default::default()
    };

    settings.merge(&update);

    assert_eq!(
        settings.default_model_id,
        Some(99),
        "Merge should update default_model_id"
    );
}

#[test]
fn test_settings_merge_clears_default_model_id() {
    let mut settings = Settings::with_defaults();
    settings.default_model_id = Some(42);

    let update = SettingsUpdate {
        default_model_id: Some(None),
        ..Default::default()
    };

    settings.merge(&update);

    assert_eq!(
        settings.default_model_id, None,
        "Merge should clear default_model_id when set to None"
    );
}
