use super::*;
use gglib_core::domain::ModelCapabilities;
use gglib_core::settings::DEFAULT_CONTEXT_SIZE;

// =========================================================================
// ErrorResponse construction tests
// =========================================================================

#[test]
fn error_response_new_sets_type_and_message() {
    let err = ErrorResponse::new("something broke", "server_error");
    assert_eq!(err.error.message, "something broke");
    assert_eq!(err.error.r#type, "server_error");
    assert!(err.error.code.is_none());
}

#[test]
fn error_response_with_code_sets_all_fields() {
    let err = ErrorResponse::with_code("bad input", "invalid_request", "bad_param");
    assert_eq!(err.error.message, "bad input");
    assert_eq!(err.error.r#type, "invalid_request");
    assert_eq!(err.error.code.as_deref(), Some("bad_param"));
}

#[test]
fn error_response_model_loading_has_correct_code() {
    let err = ErrorResponse::model_loading();
    assert_eq!(err.error.code.as_deref(), Some("model_loading"));
    assert_eq!(err.error.r#type, "service_unavailable");
    assert!(err.error.message.contains("loading"));
}

#[test]
fn error_response_model_not_found_includes_name() {
    let err = ErrorResponse::model_not_found("llama-3-8b");
    assert!(err.error.message.contains("llama-3-8b"));
    assert_eq!(err.error.code.as_deref(), Some("model_not_found"));
    assert_eq!(err.error.r#type, "invalid_request_error");
}

#[test]
fn error_response_upstream_error_includes_reason() {
    let err = ErrorResponse::upstream_error("connection refused");
    assert!(err.error.message.contains("connection refused"));
    assert_eq!(err.error.code.as_deref(), Some("upstream_error"));
    assert_eq!(err.error.r#type, "server_error");
}

// =========================================================================
// ErrorResponse serialization tests (OpenAI format compliance)
// =========================================================================

#[test]
fn error_response_serializes_to_openai_format() {
    let err = ErrorResponse::model_not_found("test-model");
    let json = serde_json::to_value(&err).unwrap();

    // OpenAI format: { "error": { "message": ..., "type": ..., "code": ... } }
    assert!(
        json.get("error").is_some(),
        "must have top-level 'error' key"
    );
    let inner = &json["error"];
    assert!(inner.get("message").is_some());
    assert!(inner.get("type").is_some());
    assert!(inner.get("code").is_some());
}

#[test]
fn error_response_without_code_omits_code_field() {
    let err = ErrorResponse::new("oops", "server_error");
    let json = serde_json::to_value(&err).unwrap();
    // code is None, so with skip_serializing_if it should be absent
    // Actually code is not skip_serializing_if on ErrorDetail, it's Option
    // Let's verify the value is null or absent
    let code = &json["error"]["code"];
    assert!(code.is_null(), "code should be null when None");
}

// =========================================================================
// ModelRuntimeError → ErrorResponse conversion tests
// =========================================================================

#[test]
fn from_model_not_found_error() {
    let err: ErrorResponse = ModelRuntimeError::ModelNotFound("qwen-7b".into()).into();
    assert!(err.error.message.contains("qwen-7b"));
    assert_eq!(err.error.code.as_deref(), Some("model_not_found"));
}

#[test]
fn from_model_loading_error() {
    let err: ErrorResponse = ModelRuntimeError::ModelLoading.into();
    assert_eq!(err.error.code.as_deref(), Some("model_loading"));
}

#[test]
fn from_spawn_failed_error() {
    let err: ErrorResponse =
        ModelRuntimeError::SpawnFailed("port already in use".into()).into();
    assert!(err.error.message.contains("port already in use"));
    assert_eq!(err.error.code.as_deref(), Some("upstream_error"));
}

#[test]
fn from_health_check_failed_error() {
    let err: ErrorResponse =
        ModelRuntimeError::HealthCheckFailed("timeout after 30s".into()).into();
    assert!(err.error.message.contains("timeout after 30s"));
    assert_eq!(err.error.code.as_deref(), Some("upstream_error"));
}

#[test]
fn from_model_file_not_found_error() {
    let err: ErrorResponse =
        ModelRuntimeError::ModelFileNotFound("/models/missing.gguf".into()).into();
    assert!(err.error.message.contains("/models/missing.gguf"));
    assert_eq!(err.error.code.as_deref(), Some("model_file_not_found"));
}

#[test]
fn from_internal_error() {
    let err: ErrorResponse = ModelRuntimeError::Internal("db locked".into()).into();
    assert_eq!(err.error.message, "db locked");
    assert_eq!(err.error.r#type, "server_error");
    assert!(err.error.code.is_none());
}

/// Wire-format contract: ContentionTimeout and ModelLoading must share the same
/// `service_unavailable` type so clients treat both as retryable with identical
/// backoff behavior.
#[test]
fn contention_timeout_and_model_loading_share_service_unavailable_type() {
    let loading: ErrorResponse = ModelRuntimeError::ModelLoading.into();
    let contention: ErrorResponse =
        ModelRuntimeError::ContentionTimeout("Insufficient time remaining".into()).into();

    // Both must be service_unavailable (HTTP 503, retryable)
    assert_eq!(loading.error.r#type, "service_unavailable");
    assert_eq!(contention.error.r#type, "service_unavailable");

    // But codes should differ so clients can distinguish the cause
    assert_eq!(loading.error.code.as_deref(), Some("model_loading"));
    assert_eq!(contention.error.code.as_deref(), Some("contention_timeout"));
}

// =========================================================================
// ModelsResponse tests
// =========================================================================

#[test]
fn models_response_from_empty_summaries() {
    let resp = ModelsResponse::from_summaries(vec![], DEFAULT_CONTEXT_SIZE);
    assert_eq!(resp.object, "list");
    assert!(resp.data.is_empty());
}

#[test]
fn models_response_from_summaries_maps_fields() {
    let summaries = vec![
        ModelSummary {
            id: 1,
            name: "llama-3-8b-q4".into(),
            tags: vec!["chat".into()],
            capabilities: ModelCapabilities::empty(),
            param_count: "8B".into(),
            quantization: Some("Q4_K_M".into()),
            architecture: Some("llama".into()),
            created_at: 1700000000,
            file_size: 4_000_000_000,
            context_length: Some(8192),
            inference_defaults: None,
            server_defaults: None,
        },
        ModelSummary {
            id: 2,
            name: "mistral-7b-q8".into(),
            tags: vec![],
            capabilities: ModelCapabilities::empty(),
            param_count: "7B".into(),
            quantization: Some("Q8_0".into()),
            architecture: Some("mistral".into()),
            created_at: 1700000001,
            file_size: 7_000_000_000,
            context_length: None,
            inference_defaults: None,
            server_defaults: None,
        },
    ];

    let resp = ModelsResponse::from_summaries(summaries, DEFAULT_CONTEXT_SIZE);
    assert_eq!(resp.data.len(), 2);
    assert_eq!(resp.data[0].id, "llama-3-8b-q4");
    assert_eq!(resp.data[0].object, "model");
    assert_eq!(resp.data[0].owned_by, "gglib");
    assert_eq!(resp.data[0].created, 1700000000);
    assert!(resp.data[0].description.is_some());
}

#[test]
fn models_response_serializes_to_openai_format() {
    let resp = ModelsResponse::from_summaries(
        vec![ModelSummary {
            id: 1,
            name: "test-model".into(),
            tags: vec![],
            capabilities: ModelCapabilities::empty(),
            param_count: "7B".into(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
            context_length: None,
            inference_defaults: None,
            server_defaults: None,
        }],
        DEFAULT_CONTEXT_SIZE,
    );

    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["object"], "list");
    assert!(json["data"].is_array());
    assert_eq!(json["data"][0]["id"], "test-model");
    assert_eq!(json["data"][0]["object"], "model");
}

// =========================================================================
// ModelInfo conversion tests
// =========================================================================

#[test]
fn model_info_description_includes_arch_and_quant() {
    let summary = ModelSummary {
        id: 1,
        name: "test".into(),
        tags: vec![],
        capabilities: ModelCapabilities::empty(),
        param_count: "13B".into(),
        quantization: Some("Q5_K_S".into()),
        architecture: Some("llama".into()),
        created_at: 0,
        file_size: 0,
        context_length: None,
        inference_defaults: None,
        server_defaults: None,
    };
    let resp = ModelsResponse::from_summaries(vec![summary], DEFAULT_CONTEXT_SIZE);
    let info = &resp.data[0];
    let desc = info.description.as_ref().unwrap();
    assert!(desc.contains("llama"), "description should include arch");
    assert!(desc.contains("13B"), "description should include params");
    assert!(desc.contains("Q5_K_S"), "description should include quant");
}

#[test]
fn model_info_handles_missing_arch_and_quant() {
    let summary = ModelSummary {
        id: 1,
        name: "bare-model".into(),
        tags: vec![],
        capabilities: ModelCapabilities::empty(),
        param_count: "1B".into(),
        quantization: None,
        architecture: None,
        created_at: 0,
        file_size: 0,
        context_length: None,
        inference_defaults: None,
        server_defaults: None,
    };
    let resp = ModelsResponse::from_summaries(vec![summary], DEFAULT_CONTEXT_SIZE);
    let info = &resp.data[0];
    let desc = info.description.as_ref().unwrap();
    assert!(
        desc.contains("unknown"),
        "missing fields should show 'unknown'"
    );
}

#[test]
fn model_info_maps_context_length_to_context_window() {
    let summary = ModelSummary {
        id: 1,
        name: "ctx-model".into(),
        tags: vec![],
        capabilities: ModelCapabilities::empty(),
        param_count: "7B".into(),
        quantization: None,
        architecture: None,
        created_at: 0,
        file_size: 0,
        context_length: Some(32_768),
        inference_defaults: None,
        server_defaults: None,
    };
    let resp = ModelsResponse::from_summaries(vec![summary], DEFAULT_CONTEXT_SIZE);
    // With no server_defaults, resolve_context_size returns global default (4096).
    // min(32768, 4096) = 4096.
    assert_eq!(resp.data[0].context_window, Some(4096));
}

#[test]
fn model_info_context_window_none_when_unknown() {
    let summary = ModelSummary {
        id: 1,
        name: "unknown-ctx-model".into(),
        tags: vec![],
        capabilities: ModelCapabilities::empty(),
        param_count: "7B".into(),
        quantization: None,
        architecture: None,
        created_at: 0,
        file_size: 0,
        context_length: None,
        inference_defaults: None,
        server_defaults: None,
    };
    let resp = ModelsResponse::from_summaries(vec![summary], DEFAULT_CONTEXT_SIZE);
    assert_eq!(resp.data[0].context_window, None);
}

#[test]
fn models_response_respects_server_defaults_context_length() {
    use gglib_core::domain::ServerConfig;

    let summary = ModelSummary {
        id: 1,
        name: "server-ctx-model".into(),
        tags: vec![],
        capabilities: ModelCapabilities::empty(),
        param_count: "7B".into(),
        quantization: None,
        architecture: None,
        created_at: 0,
        file_size: 0,
        context_length: Some(32_768), // GGUF ceiling is large
        inference_defaults: None,
        server_defaults: Some(ServerConfig {
            context_length: Some(8192),
        }),
    };
    // Global default is 4096, but server_defaults (8192) wins.
    // min(32768, 8192) = 8192.
    let resp = ModelsResponse::from_summaries(vec![summary], 4096);
    assert_eq!(resp.data[0].context_window, Some(8192));
}

#[test]
fn models_response_falls_through_when_server_defaults_context_length_none() {
    use gglib_core::domain::ServerConfig;

    let summary = ModelSummary {
        id: 1,
        name: "fallback-ctx-model".into(),
        tags: vec![],
        capabilities: ModelCapabilities::empty(),
        param_count: "7B".into(),
        quantization: None,
        architecture: None,
        created_at: 0,
        file_size: 0,
        context_length: Some(32_768),
        inference_defaults: None,
        server_defaults: Some(ServerConfig {
            context_length: None, // exists but context_length is None
        }),
    };
    // Falls through to global default (4096).
    // min(32768, 4096) = 4096.
    let resp = ModelsResponse::from_summaries(vec![summary], 4096);
    assert_eq!(resp.data[0].context_window, Some(4096));
}

// =========================================================================
// ChatCompletionRequest deserialization tests
// =========================================================================

#[test]
fn chat_request_minimal_deserializes() {
    let json = r#"{
        "model": "llama-3",
        "messages": [{"role": "user", "content": "hello"}]
    }"#;
    let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.model, "llama-3");
    assert!(!req.stream, "stream should default to false");
    assert_eq!(req.messages.len(), 1);
    assert!(req.temperature.is_none());
    assert!(req.tools.is_none());
}

#[test]
fn chat_request_with_streaming_flag() {
    let json = r#"{
        "model": "test",
        "messages": [],
        "stream": true
    }"#;
    let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
    assert!(req.stream);
}

#[test]
fn chat_request_with_all_optional_fields() {
    let json = r#"{
        "model": "llama-3",
        "messages": [{"role": "user", "content": "hi"}],
        "temperature": 0.7,
        "top_p": 0.9,
        "max_tokens": 512,
        "stream": false,
        "n": 1,
        "stop": ["END"],
        "num_ctx": 8192,
        "tools": [{
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get weather",
                "parameters": {"type": "object"}
            }
        }],
        "tool_choice": "auto"
    }"#;
    let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.temperature, Some(0.7));
    assert_eq!(req.top_p, Some(0.9));
    assert_eq!(req.max_tokens, Some(512));
    assert_eq!(req.n, Some(1));
    assert_eq!(req.stop.as_ref().unwrap().len(), 1);
    assert_eq!(req.num_ctx, Some(8192));
    assert_eq!(req.tools.as_ref().unwrap().len(), 1);
    assert_eq!(req.tools.as_ref().unwrap()[0].function.name, "get_weather");
}

#[test]
fn chat_request_with_tool_message() {
    let json = r#"{
        "model": "test",
        "messages": [
            {"role": "user", "content": "What's the weather?"},
            {"role": "assistant", "content": null, "tool_calls": [{
                "id": "call_123",
                "type": "function",
                "function": {"name": "get_weather", "arguments": "{\"city\":\"NYC\"}"}
            }]},
            {"role": "tool", "tool_call_id": "call_123", "content": "72°F sunny"}
        ]
    }"#;
    let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.messages.len(), 3);
    assert_eq!(req.messages[1].role, "assistant");
    assert!(req.messages[1].tool_calls.is_some());
    assert_eq!(req.messages[2].role, "tool");
    assert_eq!(req.messages[2].tool_call_id.as_deref(), Some("call_123"));
}

// =========================================================================
// ChatCompletionResponse serialization tests
// =========================================================================

#[test]
fn chat_response_serializes_correctly() {
    let resp = ChatCompletionResponse {
        id: "chatcmpl-123".into(),
        object: "chat.completion".into(),
        created: 1700000000,
        model: "llama-3".into(),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".into(),
                content: Some("Hello!".into()),
                tool_calls: None,
                tool_call_id: None,
            },
            finish_reason: Some("stop".into()),
        }],
        usage: Some(Usage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
        }),
    };

    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["id"], "chatcmpl-123");
    assert_eq!(json["object"], "chat.completion");
    assert_eq!(json["choices"][0]["message"]["role"], "assistant");
    assert_eq!(json["choices"][0]["message"]["content"], "Hello!");
    assert_eq!(json["choices"][0]["finish_reason"], "stop");
    assert_eq!(json["usage"]["total_tokens"], 15);
}

#[test]
fn chat_response_omits_none_usage() {
    let resp = ChatCompletionResponse {
        id: "test".into(),
        object: "chat.completion".into(),
        created: 0,
        model: "test".into(),
        choices: vec![],
        usage: None,
    };

    let json = serde_json::to_value(&resp).unwrap();
    assert!(json.get("usage").is_none() || json["usage"].is_null());
}

// =========================================================================
// Streaming chunk serialization tests
// =========================================================================

#[test]
fn chat_chunk_serializes_with_delta() {
    let chunk = ChatCompletionChunk {
        id: "chatcmpl-123".into(),
        object: "chat.completion.chunk".into(),
        created: 1700000000,
        model: "llama-3".into(),
        choices: vec![ChatChunkChoice {
            index: 0,
            delta: ChatDelta {
                role: Some("assistant".into()),
                content: Some("Hi".into()),
                tool_calls: None,
            },
            finish_reason: None,
        }],
    };

    let json = serde_json::to_value(&chunk).unwrap();
    assert_eq!(json["object"], "chat.completion.chunk");
    assert_eq!(json["choices"][0]["delta"]["content"], "Hi");
    // finish_reason should be null (not omitted) for streaming
    assert!(json["choices"][0].get("finish_reason").is_some());
}

#[test]
fn chat_chunk_with_tool_call_delta() {
    let chunk = ChatCompletionChunk {
        id: "test".into(),
        object: "chat.completion.chunk".into(),
        created: 0,
        model: "test".into(),
        choices: vec![ChatChunkChoice {
            index: 0,
            delta: ChatDelta {
                role: None,
                content: None,
                tool_calls: Some(vec![ToolCallDelta {
                    index: 0,
                    id: Some("call_abc".into()),
                    r#type: Some("function".into()),
                    function: Some(ToolCallFunctionDelta {
                        name: Some("get_weather".into()),
                        arguments: Some("{\"city\":".into()),
                    }),
                }]),
            },
            finish_reason: None,
        }],
    };

    let json = serde_json::to_value(&chunk).unwrap();
    let tc = &json["choices"][0]["delta"]["tool_calls"][0];
    assert_eq!(tc["id"], "call_abc");
    assert_eq!(tc["function"]["name"], "get_weather");
}

#[test]
fn chat_delta_omits_none_fields() {
    let delta = ChatDelta {
        role: None,
        content: None,
        tool_calls: None,
    };
    let json = serde_json::to_value(&delta).unwrap();
    // All fields have skip_serializing_if, so the JSON should be minimal
    assert!(json.get("role").is_none() || json["role"].is_null());
    assert!(json.get("content").is_none() || json["content"].is_null());
    assert!(json.get("tool_calls").is_none() || json["tool_calls"].is_null());
}

// =========================================================================
// ChatMessage serialization tests
// =========================================================================

#[test]
fn chat_message_omits_none_tool_fields() {
    let msg = ChatMessage {
        role: "user".into(),
        content: Some("hello".into()),
        tool_calls: None,
        tool_call_id: None,
    };
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["role"], "user");
    assert_eq!(json["content"], "hello");
    // tool_calls and tool_call_id should be omitted
    assert!(
        json.get("tool_calls").is_none() || json["tool_calls"].is_null(),
        "tool_calls should be omitted when None"
    );
}

#[test]
fn chat_message_with_tool_calls_serializes() {
    let msg = ChatMessage {
        role: "assistant".into(),
        content: None,
        tool_calls: Some(vec![ToolCall {
            id: "call_1".into(),
            r#type: "function".into(),
            function: ToolCallFunction {
                name: "search".into(),
                arguments: r#"{"q":"rust"}"#.into(),
            },
        }]),
        tool_call_id: None,
    };
    let json = serde_json::to_value(&msg).unwrap();
    assert!(json.get("content").is_none() || json["content"].is_null());
    assert_eq!(json["tool_calls"][0]["id"], "call_1");
    assert_eq!(json["tool_calls"][0]["function"]["name"], "search");
}

// =========================================================================
// ChatRoutingEnvelope tests
// =========================================================================

#[test]
fn routing_envelope_extracts_model_stream_num_ctx() {
    let json = r#"{
        "model": "llama-3",
        "stream": true,
        "num_ctx": 16384,
        "messages": [{"role": "user", "content": "hello"}],
        "temperature": 0.7
    }"#;
    let env: ChatRoutingEnvelope = serde_json::from_str(json).unwrap();
    assert_eq!(env.model, "llama-3");
    assert!(env.stream);
    assert_eq!(env.num_ctx, Some(16384));
}

#[test]
fn routing_envelope_stream_defaults_false() {
    let json = r#"{"model": "test", "messages": []}"#;
    let env: ChatRoutingEnvelope = serde_json::from_str(json).unwrap();
    assert!(!env.stream);
    assert!(env.num_ctx.is_none());
}

/// Regression test for #438: content as an array of content parts must not
/// cause a 400 from the proxy. The routing envelope ignores `messages`
/// entirely, so any valid-JSON content form passes through.
#[test]
fn routing_envelope_accepts_array_form_content() {
    let json = r#"{
        "model": "gpt-4o",
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "Hello"},
                    {"type": "image_url", "image_url": {"url": "https://example.com/img.png"}}
                ]
            }
        ]
    }"#;
    let env: ChatRoutingEnvelope = serde_json::from_str(json).unwrap();
    assert_eq!(env.model, "gpt-4o");
}

/// Regression test for #438: stop as a bare string (valid per OpenAI spec)
/// must not cause a 400.
#[test]
fn routing_envelope_accepts_stop_as_bare_string() {
    let json = r#"{"model": "test", "messages": [], "stop": "END"}"#;
    let env: ChatRoutingEnvelope = serde_json::from_str(json).unwrap();
    assert_eq!(env.model, "test");
}

#[test]
fn routing_envelope_rejects_missing_model() {
    let json = r#"{"messages": [], "stream": false}"#;
    let result: Result<ChatRoutingEnvelope, _> = serde_json::from_str(json);
    assert!(result.is_err(), "model is required");
}
