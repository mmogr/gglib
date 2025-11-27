//! Unit tests for proxy handler functionality.
//!
//! Note: Full integration tests for the proxy endpoints require running
//! llama-server, which is covered in integration tests. These unit tests
//! focus on testable components like request parsing and response building.

use gglib::models::Gguf;
use gglib::services::database;
use std::path::PathBuf;

use crate::common::database::setup_test_pool;

/// Helper to create a test model for proxy tests
fn create_proxy_test_model(name: &str) -> Gguf {
    let mut model = Gguf::new(
        name.to_string(),
        PathBuf::from(format!("/models/{}.gguf", name)),
        7.0,
        chrono::Utc::now(),
    );
    model.architecture = Some("llama".to_string());
    model.quantization = Some("Q4_K_M".to_string());
    model.context_length = Some(4096);
    model
}

/// Test that models can be listed from database (simulating /v1/models endpoint)
#[tokio::test]
async fn test_list_models_from_database() {
    let pool = setup_test_pool().await.unwrap();

    // Add some models
    let model1 = create_proxy_test_model("llama-7b");
    let model2 = create_proxy_test_model("mistral-7b");

    database::add_model(&pool, &model1).await.unwrap();
    database::add_model(&pool, &model2).await.unwrap();

    // List models (simulating what the handler does)
    let models = database::list_models(&pool).await.unwrap();

    assert_eq!(models.len(), 2);

    // Convert to ModelInfo like the handler does
    let model_infos: Vec<gglib::proxy::models::ModelInfo> = models
        .into_iter()
        .map(|m| gglib::proxy::models::ModelInfo {
            id: m.name.clone(),
            object: "model".to_string(),
            created: m.added_at.timestamp(),
            owned_by: "gglib".to_string(),
            description: Some(format!(
                "{} - {} parameters, {}",
                m.architecture.as_deref().unwrap_or("unknown"),
                m.param_count_b,
                m.quantization.as_deref().unwrap_or("unknown quant")
            )),
        })
        .collect();

    assert_eq!(model_infos.len(), 2);

    // Check first model (should be mistral-7b due to ORDER BY added_at DESC)
    let first = &model_infos[0];
    assert_eq!(first.object, "model");
    assert_eq!(first.owned_by, "gglib");
    assert!(first.description.as_ref().unwrap().contains("llama"));
    assert!(first.description.as_ref().unwrap().contains("7"));
    assert!(first.description.as_ref().unwrap().contains("Q4_K_M"));
}

/// Test models response structure matches OpenAI format
#[tokio::test]
async fn test_models_response_format() {
    let pool = setup_test_pool().await.unwrap();

    let model = create_proxy_test_model("test-model");
    database::add_model(&pool, &model).await.unwrap();

    let models = database::list_models(&pool).await.unwrap();

    let response = gglib::proxy::models::ModelsResponse {
        object: "list".to_string(),
        data: models
            .into_iter()
            .map(|m| gglib::proxy::models::ModelInfo {
                id: m.name.clone(),
                object: "model".to_string(),
                created: m.added_at.timestamp(),
                owned_by: "gglib".to_string(),
                description: None,
            })
            .collect(),
    };

    // Serialize to JSON and verify structure
    let json = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["object"], "list");
    assert!(parsed["data"].is_array());
    assert_eq!(parsed["data"][0]["id"], "test-model");
    assert_eq!(parsed["data"][0]["object"], "model");
}

/// Test empty models list
#[tokio::test]
async fn test_list_models_empty_database() {
    let pool = setup_test_pool().await.unwrap();

    let models = database::list_models(&pool).await.unwrap();

    assert!(models.is_empty());

    let response = gglib::proxy::models::ModelsResponse {
        object: "list".to_string(),
        data: vec![],
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["object"], "list");
    assert_eq!(parsed["data"].as_array().unwrap().len(), 0);
}

/// Test model lookup by name (used by chat_completions handler)
#[tokio::test]
async fn test_find_model_for_chat() {
    let pool = setup_test_pool().await.unwrap();

    let model = create_proxy_test_model("my-llama-model");
    database::add_model(&pool, &model).await.unwrap();

    // The handler uses find_model_by_identifier to look up models
    let found = database::find_model_by_identifier(&pool, "my-llama-model")
        .await
        .unwrap();

    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "my-llama-model");
    assert_eq!(found.param_count_b, 7.0);
}

/// Test model not found scenario
#[tokio::test]
async fn test_model_not_found_for_chat() {
    let pool = setup_test_pool().await.unwrap();

    let found = database::find_model_by_identifier(&pool, "nonexistent-model")
        .await
        .unwrap();

    assert!(found.is_none());

    // Verify error response format
    let error = gglib::proxy::models::ErrorResponse::new(
        "Model 'nonexistent-model' not found",
        "model_not_found",
    );

    let json = serde_json::to_string(&error).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(
        parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("nonexistent-model")
    );
    assert_eq!(parsed["error"]["type"], "model_not_found");
}

/// Test request validation - valid chat completion request
#[test]
fn test_valid_chat_request_parsing() {
    let request_body = serde_json::json!({
        "model": "llama-7b",
        "messages": [
            {"role": "system", "content": "You are helpful."},
            {"role": "user", "content": "Hello!"}
        ],
        "temperature": 0.7,
        "stream": false
    });

    let request: Result<gglib::proxy::models::ChatCompletionRequest, _> =
        serde_json::from_value(request_body);

    assert!(request.is_ok());
    let req = request.unwrap();
    assert_eq!(req.model, "llama-7b");
    assert_eq!(req.messages.len(), 2);
    assert!(!req.stream);
}

/// Test request validation - streaming request
#[test]
fn test_streaming_chat_request() {
    let request_body = serde_json::json!({
        "model": "llama-7b",
        "messages": [{"role": "user", "content": "Hello!"}],
        "stream": true
    });

    let request: gglib::proxy::models::ChatCompletionRequest =
        serde_json::from_value(request_body).unwrap();

    assert!(request.stream);
}

/// Test request validation - with num_ctx (Ollama compatibility)
#[test]
fn test_chat_request_with_num_ctx() {
    let request_body = serde_json::json!({
        "model": "llama-7b",
        "messages": [{"role": "user", "content": "Hello!"}],
        "num_ctx": 8192
    });

    let request: gglib::proxy::models::ChatCompletionRequest =
        serde_json::from_value(request_body).unwrap();

    assert_eq!(request.num_ctx, Some(8192));
}

/// Test error response for invalid request body
#[test]
fn test_invalid_request_body_error() {
    let invalid_body = serde_json::json!({
        "model": "llama-7b"
        // Missing required "messages" field
    });

    let result: Result<gglib::proxy::models::ChatCompletionRequest, _> =
        serde_json::from_value(invalid_body);

    assert!(result.is_err());

    // The handler would return this error
    let error = gglib::proxy::models::ErrorResponse::new("Invalid request body", "invalid_request");

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("Invalid request body"));
}

/// Test error response for upstream communication failure
#[test]
fn test_upstream_error_response() {
    let error = gglib::proxy::models::ErrorResponse::new(
        "Failed to communicate with llama-server: Connection refused",
        "upstream_error",
    );

    let json = serde_json::to_string(&error).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(
        parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Connection refused")
    );
    assert_eq!(parsed["error"]["type"], "upstream_error");
}

/// Test model description generation
#[tokio::test]
async fn test_model_description_format() {
    let pool = setup_test_pool().await.unwrap();

    let mut model = create_proxy_test_model("test-model");
    model.architecture = Some("mistral".to_string());
    model.param_count_b = 7.5;
    model.quantization = Some("Q5_K_S".to_string());

    database::add_model(&pool, &model).await.unwrap();

    let models = database::list_models(&pool).await.unwrap();
    let m = &models[0];

    let description = format!(
        "{} - {} parameters, {}",
        m.architecture.as_deref().unwrap_or("unknown"),
        m.param_count_b,
        m.quantization.as_deref().unwrap_or("unknown quant")
    );

    assert_eq!(description, "mistral - 7.5 parameters, Q5_K_S");
}

/// Test model description with missing fields
#[tokio::test]
async fn test_model_description_missing_fields() {
    let pool = setup_test_pool().await.unwrap();

    // Create model with minimal fields
    let model = Gguf::new(
        "minimal-model".to_string(),
        PathBuf::from("/test/minimal.gguf"),
        3.0,
        chrono::Utc::now(),
    );

    database::add_model(&pool, &model).await.unwrap();

    let models = database::list_models(&pool).await.unwrap();
    let m = &models[0];

    let description = format!(
        "{} - {} parameters, {}",
        m.architecture.as_deref().unwrap_or("unknown"),
        m.param_count_b,
        m.quantization.as_deref().unwrap_or("unknown quant")
    );

    assert_eq!(description, "unknown - 3 parameters, unknown quant");
}

/// Test multiple models ordering
#[tokio::test]
async fn test_models_ordered_by_added_at() {
    let pool = setup_test_pool().await.unwrap();

    // Add models in sequence
    let model1 = create_proxy_test_model("first-model");
    database::add_model(&pool, &model1).await.unwrap();

    // Small delay to ensure different timestamps
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let model2 = create_proxy_test_model("second-model");
    database::add_model(&pool, &model2).await.unwrap();

    let models = database::list_models(&pool).await.unwrap();

    // Should be ordered by added_at DESC (most recent first)
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].name, "second-model");
    assert_eq!(models[1].name, "first-model");
}
