//! Unit tests for proxy API models serialization and deserialization.

/// Test `ChatCompletionRequest` deserialization
#[test]
fn test_chat_completion_request_deserialize_minimal() {
    let json_str = r#"{
        "model": "llama-7b",
        "messages": [
            {"role": "user", "content": "Hello!"}
        ]
    }"#;

    let request: gglib_runtime::proxy::models::ChatCompletionRequest =
        serde_json::from_str(json_str).unwrap();

    assert_eq!(request.model, "llama-7b");
    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.messages[0].role, "user");
    assert_eq!(request.messages[0].content, Some("Hello!".to_string()));
    assert!(!request.stream); // Default should be false
    assert!(request.temperature.is_none());
    assert!(request.top_p.is_none());
    assert!(request.max_tokens.is_none());
    assert!(request.num_ctx.is_none());
}

#[test]
fn test_chat_completion_request_deserialize_full() {
    let json_str = r#"{
        "model": "mistral-7b",
        "messages": [
            {"role": "system", "content": "You are a helpful assistant."},
            {"role": "user", "content": "What is Rust?"},
            {"role": "assistant", "content": "Rust is a systems programming language."},
            {"role": "user", "content": "Tell me more."}
        ],
        "temperature": 0.7,
        "top_p": 0.9,
        "max_tokens": 1024,
        "stream": true,
        "n": 1,
        "stop": ["END", "STOP"],
        "num_ctx": 8192
    }"#;

    let request: gglib_runtime::proxy::models::ChatCompletionRequest =
        serde_json::from_str(json_str).unwrap();

    assert_eq!(request.model, "mistral-7b");
    assert_eq!(request.messages.len(), 4);
    assert_eq!(request.messages[0].role, "system");
    assert!(request.stream);
    assert_eq!(request.temperature, Some(0.7));
    assert_eq!(request.top_p, Some(0.9));
    assert_eq!(request.max_tokens, Some(1024));
    assert_eq!(request.n, Some(1));
    assert_eq!(
        request.stop,
        Some(vec!["END".to_string(), "STOP".to_string()])
    );
    assert_eq!(request.num_ctx, Some(8192));
}

#[test]
fn test_chat_completion_request_stream_defaults_false() {
    let json_str = r#"{
        "model": "test-model",
        "messages": []
    }"#;

    let request: gglib_runtime::proxy::models::ChatCompletionRequest =
        serde_json::from_str(json_str).unwrap();

    assert!(!request.stream);
}

/// Test `ChatMessage` serialization roundtrip
#[test]
fn test_chat_message_serialize_deserialize() {
    let message = gglib_runtime::proxy::models::ChatMessage {
        role: "assistant".to_string(),
        content: Some("Hello, how can I help you?".to_string()),
        tool_calls: None,
        tool_call_id: None,
    };

    let json = serde_json::to_string(&message).unwrap();
    let deserialized: gglib_runtime::proxy::models::ChatMessage =
        serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.role, "assistant");
    assert_eq!(
        deserialized.content,
        Some("Hello, how can I help you?".to_string())
    );
}

#[test]
fn test_chat_message_with_unicode() {
    let message = gglib_runtime::proxy::models::ChatMessage {
        role: "user".to_string(),
        content: Some("Hello! 你好 🦙 émojis работает".to_string()),
        tool_calls: None,
        tool_call_id: None,
    };

    let json = serde_json::to_string(&message).unwrap();
    let deserialized: gglib_runtime::proxy::models::ChatMessage =
        serde_json::from_str(&json).unwrap();

    assert_eq!(
        deserialized.content,
        Some("Hello! 你好 🦙 émojis работает".to_string())
    );
}

/// Test `ModelsResponse` serialization
#[test]
fn test_models_response_serialize() {
    let response = gglib_runtime::proxy::models::ModelsResponse {
        object: "list".to_string(),
        data: vec![
            gglib_runtime::proxy::models::ModelInfo {
                id: "llama-7b".to_string(),
                object: "model".to_string(),
                created: 1700000000,
                owned_by: "gglib".to_string(),
                description: Some("A 7B parameter model".to_string()),
            },
            gglib_runtime::proxy::models::ModelInfo {
                id: "mistral-7b".to_string(),
                object: "model".to_string(),
                created: 1700000001,
                owned_by: "gglib".to_string(),
                description: None,
            },
        ],
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["object"], "list");
    assert_eq!(parsed["data"].as_array().unwrap().len(), 2);
    assert_eq!(parsed["data"][0]["id"], "llama-7b");
    assert_eq!(parsed["data"][0]["description"], "A 7B parameter model");
    // Second model has no description, should be omitted
    assert!(parsed["data"][1]["description"].is_null());
}

/// Test `ModelInfo` serialization
#[test]
fn test_model_info_serialize() {
    let info = gglib_runtime::proxy::models::ModelInfo {
        id: "test-model".to_string(),
        object: "model".to_string(),
        created: 1234567890,
        owned_by: "gglib".to_string(),
        description: Some("Test description".to_string()),
    };

    let json = serde_json::to_string(&info).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["id"], "test-model");
    assert_eq!(parsed["object"], "model");
    assert_eq!(parsed["created"], 1234567890);
    assert_eq!(parsed["owned_by"], "gglib");
    assert_eq!(parsed["description"], "Test description");
}

/// Test `ErrorResponse` construction and serialization
#[test]
fn test_error_response_new() {
    let error = gglib_runtime::proxy::models::ErrorResponse::new("Model not found", "not_found");

    let json = serde_json::to_string(&error).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["error"]["message"], "Model not found");
    assert_eq!(parsed["error"]["type"], "not_found");
    assert!(parsed["error"]["code"].is_null());
}

#[test]
fn test_error_response_with_string_conversion() {
    let error = gglib_runtime::proxy::models::ErrorResponse::new(
        String::from("Connection failed"),
        String::from("connection_error"),
    );

    let json = serde_json::to_string(&error).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["error"]["message"], "Connection failed");
    assert_eq!(parsed["error"]["type"], "connection_error");
}

/// Test `ChatCompletionResponse` serialization
#[test]
fn test_chat_completion_response_serialize() {
    let response = gglib_runtime::proxy::models::ChatCompletionResponse {
        id: "chatcmpl-123".to_string(),
        object: "chat.completion".to_string(),
        created: 1700000000,
        model: "llama-7b".to_string(),
        choices: vec![gglib_runtime::proxy::models::ChatChoice {
            index: 0,
            message: gglib_runtime::proxy::models::ChatMessage {
                role: "assistant".to_string(),
                content: Some("Hello!".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            finish_reason: Some("stop".to_string()),
        }],
        usage: Some(gglib_runtime::proxy::models::Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
        }),
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["id"], "chatcmpl-123");
    assert_eq!(parsed["object"], "chat.completion");
    assert_eq!(parsed["choices"][0]["message"]["content"], "Hello!");
    assert_eq!(parsed["usage"]["total_tokens"], 15);
}

/// Test `ChatCompletionChunk` (streaming response) serialization
#[test]
fn test_chat_completion_chunk_serialize() {
    let chunk = gglib_runtime::proxy::models::ChatCompletionChunk {
        id: "chatcmpl-123".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1700000000,
        model: "llama-7b".to_string(),
        choices: vec![gglib_runtime::proxy::models::ChatChunkChoice {
            index: 0,
            delta: gglib_runtime::proxy::models::ChatDelta {
                role: None,
                content: Some("Hello".to_string()),
                tool_calls: None,
            },
            finish_reason: None,
        }],
    };

    let json = serde_json::to_string(&chunk).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["object"], "chat.completion.chunk");
    assert_eq!(parsed["choices"][0]["delta"]["content"], "Hello");
    // role should be omitted when None
    assert!(parsed["choices"][0]["delta"]["role"].is_null());
}

#[test]
fn test_chat_delta_with_role() {
    let delta = gglib_runtime::proxy::models::ChatDelta {
        role: Some("assistant".to_string()),
        content: None,
        tool_calls: None,
    };

    let json = serde_json::to_string(&delta).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["role"], "assistant");
    // content should be omitted when None
    assert!(parsed["content"].is_null());
}

/// Test Usage serialization roundtrip
#[test]
fn test_usage_serialize_deserialize() {
    let usage = gglib_runtime::proxy::models::Usage {
        prompt_tokens: 100,
        completion_tokens: 50,
        total_tokens: 150,
    };

    let json = serde_json::to_string(&usage).unwrap();
    let deserialized: gglib_runtime::proxy::models::Usage = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.prompt_tokens, 100);
    assert_eq!(deserialized.completion_tokens, 50);
    assert_eq!(deserialized.total_tokens, 150);
}

/// Test invalid JSON handling
#[test]
fn test_chat_completion_request_invalid_json() {
    let invalid_json = r#"{"model": "test", "messages": "not an array"}"#;

    let result: Result<gglib_runtime::proxy::models::ChatCompletionRequest, _> =
        serde_json::from_str(invalid_json);

    assert!(result.is_err());
}

#[test]
fn test_chat_completion_request_missing_required_field() {
    let missing_model = r#"{"messages": []}"#;

    let result: Result<gglib_runtime::proxy::models::ChatCompletionRequest, _> =
        serde_json::from_str(missing_model);

    assert!(result.is_err());
}

/// Test edge cases
#[test]
fn test_chat_message_empty_content() {
    let json_str = r#"{"role": "user", "content": ""}"#;

    let message: gglib_runtime::proxy::models::ChatMessage =
        serde_json::from_str(json_str).unwrap();

    assert_eq!(message.role, "user");
    assert_eq!(message.content, Some("".to_string()));
}

#[test]
fn test_chat_completion_request_empty_messages() {
    let json_str = r#"{"model": "test", "messages": []}"#;

    let request: gglib_runtime::proxy::models::ChatCompletionRequest =
        serde_json::from_str(json_str).unwrap();

    assert!(request.messages.is_empty());
}

#[test]
fn test_model_info_without_description() {
    let info = gglib_runtime::proxy::models::ModelInfo {
        id: "model".to_string(),
        object: "model".to_string(),
        created: 0,
        owned_by: "test".to_string(),
        description: None,
    };

    let json = serde_json::to_string(&info).unwrap();

    // description should be omitted when None (skip_serializing_if)
    assert!(!json.contains("description"));
}

/// Test `ChatCompletionResponse` deserialization
#[test]
fn test_chat_completion_response_deserialize() {
    let json_str = r#"{
        "id": "chatcmpl-456",
        "object": "chat.completion",
        "created": 1700000000,
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Test response"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 20,
            "completion_tokens": 10,
            "total_tokens": 30
        }
    }"#;

    let response: gglib_runtime::proxy::models::ChatCompletionResponse =
        serde_json::from_str(json_str).unwrap();

    assert_eq!(response.id, "chatcmpl-456");
    assert_eq!(response.choices.len(), 1);
    assert_eq!(
        response.choices[0].message.content,
        Some("Test response".to_string())
    );
    assert!(response.usage.is_some());
    assert_eq!(response.usage.unwrap().total_tokens, 30);
}
