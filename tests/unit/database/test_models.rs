//! Unit tests for model CRUD operations.

use gglib::models::Gguf;
use gglib::services::database::{
    self, ModelStoreError, add_model, find_model_by_identifier, find_models_by_name,
    get_model_by_id, list_models, remove_model_by_id, update_model,
};

#[path = "../../common/mod.rs"]
mod common;

use common::database::setup_test_pool;
use common::fixtures::{create_minimal_model, create_test_model, create_model_with_metadata};
use std::collections::HashMap;
use std::path::PathBuf;
use chrono::Utc;

#[tokio::test]
async fn test_add_model() {
    let pool = setup_test_pool().await.unwrap();
    let model = create_test_model("test_model");

    let result = add_model(&pool, &model).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_add_model_rejects_duplicates() {
    let pool = setup_test_pool().await.unwrap();
    let model = create_test_model("dup_model");

    add_model(&pool, &model).await.unwrap();
    let err = add_model(&pool, &model)
        .await
        .expect_err("expected duplicate error");

    let duplicate = err.downcast::<ModelStoreError>().unwrap();
    match duplicate {
        ModelStoreError::DuplicateModel { file_path, .. } => {
            assert!(file_path.contains("dup_model"));
        }
        _ => panic!("Expected DuplicateModel error"),
    }
}

#[tokio::test]
async fn test_list_models() {
    let pool = setup_test_pool().await.unwrap();
    let model1 = create_test_model("model1");
    let model2 = create_test_model("model2");

    add_model(&pool, &model1).await.unwrap();
    add_model(&pool, &model2).await.unwrap();

    let models = list_models(&pool).await.unwrap();
    assert_eq!(models.len(), 2);

    // Should be ordered by added_at DESC
    assert_eq!(models[1].name, "model1");
    assert_eq!(models[0].name, "model2");
}

#[tokio::test]
async fn test_find_models_by_name() {
    let pool = setup_test_pool().await.unwrap();
    let model1 = create_test_model("llama-7b-chat");
    let model2 = create_test_model("mistral-7b");
    let model3 = create_test_model("llama-13b");

    add_model(&pool, &model1).await.unwrap();
    add_model(&pool, &model2).await.unwrap();
    add_model(&pool, &model3).await.unwrap();

    let llama_models = find_models_by_name(&pool, "llama").await.unwrap();
    assert_eq!(llama_models.len(), 2);

    let mistral_models = find_models_by_name(&pool, "mistral").await.unwrap();
    assert_eq!(mistral_models.len(), 1);
    assert_eq!(mistral_models[0].name, "mistral-7b");
}

#[tokio::test]
async fn test_get_model_by_id_success() {
    let pool = setup_test_pool().await.unwrap();
    let model = create_test_model("get_by_id_test");

    add_model(&pool, &model).await.unwrap();

    let models = list_models(&pool).await.unwrap();
    assert_eq!(models.len(), 1);
    let model_id = models[0].id.unwrap();

    let retrieved_model = get_model_by_id(&pool, model_id).await.unwrap();
    assert!(retrieved_model.is_some());

    let retrieved_model = retrieved_model.unwrap();
    assert_eq!(retrieved_model.name, "get_by_id_test");
    assert_eq!(retrieved_model.id, Some(model_id));
    assert_eq!(retrieved_model.param_count_b, 7.0);
}

#[tokio::test]
async fn test_get_model_by_id_not_found() {
    let pool = setup_test_pool().await.unwrap();

    let result = get_model_by_id(&pool, 999).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_find_model_by_identifier_by_name() {
    let pool = setup_test_pool().await.unwrap();
    let model = create_test_model("find-by-name");

    add_model(&pool, &model).await.unwrap();

    let found = find_model_by_identifier(&pool, "find-by-name").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "find-by-name");
}

#[tokio::test]
async fn test_find_model_by_identifier_by_id() {
    let pool = setup_test_pool().await.unwrap();
    let model = create_test_model("find-by-id");

    add_model(&pool, &model).await.unwrap();
    let models = list_models(&pool).await.unwrap();
    let model_id = models[0].id.unwrap();

    let found = find_model_by_identifier(&pool, &model_id.to_string()).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "find-by-id");
}

#[tokio::test]
async fn test_find_model_by_identifier_not_found() {
    let pool = setup_test_pool().await.unwrap();

    let found = find_model_by_identifier(&pool, "nonexistent").await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn test_update_model_success() {
    let pool = setup_test_pool().await.unwrap();
    let original_model = create_test_model("update_test_original");

    add_model(&pool, &original_model).await.unwrap();

    let models = list_models(&pool).await.unwrap();
    let model_id = models[0].id.unwrap();

    let mut updated_metadata = HashMap::new();
    updated_metadata.insert("general.updated".to_string(), "true".to_string());

    let mut updated_model = Gguf::new(
        "update_test_modified".to_string(),
        PathBuf::from("/test/updated.gguf"),
        13.0,
        Utc::now(),
    );
    updated_model.id = Some(model_id);
    updated_model.architecture = Some("llama".to_string());
    updated_model.quantization = Some("Q8_0".to_string());
    updated_model.context_length = Some(8192);
    updated_model.metadata = updated_metadata;

    let result = update_model(&pool, model_id, &updated_model).await;
    assert!(result.is_ok());

    let retrieved_model = get_model_by_id(&pool, model_id).await.unwrap().unwrap();
    assert_eq!(retrieved_model.name, "update_test_modified");
    assert_eq!(retrieved_model.param_count_b, 13.0);
    assert_eq!(retrieved_model.architecture, Some("llama".to_string()));
    assert_eq!(retrieved_model.quantization, Some("Q8_0".to_string()));
    assert_eq!(retrieved_model.context_length, Some(8192));
}

#[tokio::test]
async fn test_update_model_nonexistent_id_returns_error() {
    let pool = setup_test_pool().await.unwrap();
    let dummy_model = create_test_model("dummy");

    let result = update_model(&pool, 999, &dummy_model).await;
    assert!(result.is_err());
    
    let err = result.unwrap_err();
    let store_err = err.downcast::<ModelStoreError>().unwrap();
    match store_err {
        ModelStoreError::NotFound { id } => assert_eq!(id, 999),
        _ => panic!("Expected NotFound error"),
    }
}

#[tokio::test]
async fn test_remove_model_by_id() {
    let pool = setup_test_pool().await.unwrap();
    let model = create_test_model("remove_by_id");

    add_model(&pool, &model).await.unwrap();
    let models_before = list_models(&pool).await.unwrap();
    let model_id = models_before[0].id.unwrap();

    let result = remove_model_by_id(&pool, model_id).await;
    assert!(result.is_ok());

    let models_after = list_models(&pool).await.unwrap();
    assert!(models_after.is_empty());
}

#[tokio::test]
async fn test_remove_model_by_id_missing() {
    let pool = setup_test_pool().await.unwrap();
    let result = remove_model_by_id(&pool, 42).await;
    assert!(result.is_err());
    
    let err = result.unwrap_err();
    let store_err = err.downcast::<ModelStoreError>().unwrap();
    match store_err {
        ModelStoreError::NotFound { id } => assert_eq!(id, 42),
        _ => panic!("Expected NotFound error"),
    }
}

#[tokio::test]
async fn test_model_with_metadata_serialization() {
    let pool = setup_test_pool().await.unwrap();
    let mut metadata = HashMap::new();
    metadata.insert("general.name".to_string(), "Test Model".to_string());
    metadata.insert("llama.context_length".to_string(), "4096".to_string());
    metadata.insert("general.architecture".to_string(), "llama".to_string());

    let model = create_model_with_metadata("metadata_test", metadata);
    add_model(&pool, &model).await.unwrap();

    let retrieved_models = list_models(&pool).await.unwrap();
    assert_eq!(retrieved_models.len(), 1);

    let retrieved_model = &retrieved_models[0];
    assert_eq!(retrieved_model.metadata.len(), 3);
    assert_eq!(
        retrieved_model.metadata.get("general.name"),
        Some(&"Test Model".to_string())
    );
}

#[tokio::test]
async fn test_model_with_optional_fields() {
    let pool = setup_test_pool().await.unwrap();
    let model = create_minimal_model("minimal_model");

    add_model(&pool, &model).await.unwrap();

    let retrieved_models = list_models(&pool).await.unwrap();
    assert_eq!(retrieved_models.len(), 1);

    let retrieved_model = &retrieved_models[0];
    assert_eq!(retrieved_model.name, "minimal_model");
    assert_eq!(retrieved_model.architecture, None);
    assert_eq!(retrieved_model.quantization, None);
    assert_eq!(retrieved_model.context_length, None);
    assert!(retrieved_model.metadata.is_empty());
}
