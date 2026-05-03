//! OpenAI API data models for request/response handling.
//!
//! This module contains types that match the OpenAI API specification.
//! Domain types live in `gglib-core`; this module handles the API layer mapping.

use gglib_core::ports::{ModelRuntimeError, ModelSummary};
use serde::{Deserialize, Serialize};

// =============================================================================
// Tool Calling Types
// =============================================================================

/// Tool definition for function calling (OpenAI-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool type - always "function".
    pub r#type: String,
    /// Function definition.
    pub function: FunctionDefinition,
}

/// Function definition within a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name.
    pub name: String,
    /// Description of what the function does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for function parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// A tool call made by the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this tool call.
    pub id: String,
    /// Tool type - always "function".
    pub r#type: String,
    /// Function call details.
    pub function: ToolCallFunction,
}

/// Function call details within a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    /// Name of the function to call.
    pub name: String,
    /// JSON string of arguments.
    pub arguments: String,
}

/// Streaming delta for tool calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    /// Index of the tool call (for parallel tool calls).
    pub index: u32,
    /// Tool call ID (sent in first chunk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Tool type (sent in first chunk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// Function delta.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<ToolCallFunctionDelta>,
}

/// Streaming delta for function details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunctionDelta {
    /// Function name (sent in first chunk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Partial arguments JSON string (accumulated across chunks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

// =============================================================================
// Chat Completion Request/Response Types
// =============================================================================

/// Minimal routing envelope extracted from inbound `/v1/chat/completions` requests.
///
/// The proxy only needs three fields to route a request to the correct
/// llama-server instance. Everything else in the body — message content,
/// sampling parameters, tool definitions, stop sequences, etc. — is forwarded
/// verbatim as raw bytes and is llama-server's responsibility to validate.
///
/// By deserialising into this narrow struct instead of the full
/// [`ChatCompletionRequest`], the proxy is immune to any OpenAI field whose
/// type doesn't match our local Rust types: `content` as an array of content
/// parts, `stop` as a bare string, future extensions like `reasoning_effort`,
/// audio inputs, etc.
///
/// Unknown fields are silently ignored by serde (default behaviour without
/// `deny_unknown_fields`).
#[derive(Debug, Deserialize)]
pub(crate) struct ChatRoutingEnvelope {
    /// Model name or ID used to select the llama-server instance.
    pub model: String,
    /// Whether the client expects a streaming SSE response.
    #[serde(default)]
    pub stream: bool,
    /// Optional context window override (Ollama-compatible).
    pub num_ctx: Option<u64>,
}

/// OpenAI-compatible `stop` field representation.
///
/// The OpenAI API accepts either a single string or an array of strings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum StopSequences {
    Single(String),
    Multiple(Vec<String>),
}

/// Full OpenAI-compatible chat completion request.
///
/// This type is kept for response construction, testing, and documentation
/// purposes. It is **not** used to parse inbound proxy requests — see
/// [`ChatRoutingEnvelope`] for that.
///
/// # Note on `content`
///
/// `ChatMessage.content` is typed as `Option<String>` here. The OpenAI API
/// also allows an array of content parts; callers constructing this type
/// should use `content: None` plus `tool_calls` for tool-only messages.
/// Inbound array-form content passes through the proxy untouched because the
/// proxy never deserialises it into this struct.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionRequest {
    /// Model name to use.
    pub model: String,
    /// Array of chat messages.
    pub messages: Vec<ChatMessage>,
    /// Sampling temperature (0-2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p sampling parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Maximum tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Whether to stream the response.
    #[serde(default)]
    pub stream: bool,
    /// Number of completions to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// Stop sequences (`"END"` or `["END", "STOP"]`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<StopSequences>,
    /// Context window size (Ollama-compatible parameter).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_ctx: Option<u64>,
    /// Tool definitions for function calling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    /// Tool choice: "auto", "none", "required", or specific tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
}

/// A single chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role: "system", "user", "assistant", or "tool".
    pub role: String,
    /// Message content (optional when tool_calls present).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool calls made by assistant (role="assistant" only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Tool call ID this message is responding to (role="tool" only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Response from /v1/chat/completions endpoint (non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// A single chat completion choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Streaming chunk from /v1/chat/completions endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChunkChoice>,
}

/// A single streaming choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChunkChoice {
    pub index: u32,
    pub delta: ChatDelta,
    // finish_reason must serialize as `null` for intermediate chunks per the
    // OpenAI streaming spec — intentionally NOT using skip_serializing_if here.
    pub finish_reason: Option<String>,
}

/// Delta content in streaming response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Streaming tool calls (accumulated by index).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// =============================================================================
// Models Endpoint Types
// =============================================================================

/// Response from /v1/models endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<ModelInfo>,
}

impl ModelsResponse {
    /// Create a new ModelsResponse from a list of model summaries.
    pub fn from_summaries(summaries: Vec<ModelSummary>) -> Self {
        Self {
            object: "list".to_string(),
            data: summaries.into_iter().map(ModelInfo::from).collect(),
        }
    }
}

/// Information about a single model (OpenAI format).
#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl From<ModelSummary> for ModelInfo {
    fn from(summary: ModelSummary) -> Self {
        Self {
            id: summary.name.clone(),
            object: "model".to_string(),
            created: summary.created_at,
            owned_by: "gglib".to_string(),
            description: Some(summary.description()),
        }
    }
}

// =============================================================================
// Error Response Types
// =============================================================================

/// Error response matching OpenAI format.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

/// Error detail within an error response.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorDetail {
    pub message: String,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl ErrorResponse {
    /// Create a new error response.
    pub fn new(message: impl Into<String>, error_type: impl Into<String>) -> Self {
        Self {
            error: ErrorDetail {
                message: message.into(),
                r#type: error_type.into(),
                code: None,
            },
        }
    }

    /// Create an error response with a code.
    pub fn with_code(
        message: impl Into<String>,
        error_type: impl Into<String>,
        code: impl Into<String>,
    ) -> Self {
        Self {
            error: ErrorDetail {
                message: message.into(),
                r#type: error_type.into(),
                code: Some(code.into()),
            },
        }
    }

    /// Create an error response for model loading.
    pub fn model_loading() -> Self {
        Self::with_code(
            "Model is currently loading, please retry",
            "service_unavailable",
            "model_loading",
        )
    }

    /// Create an error response for model not found.
    pub fn model_not_found(model: &str) -> Self {
        Self::with_code(
            format!("Model '{model}' not found"),
            "invalid_request_error",
            "model_not_found",
        )
    }

    /// Create an error response for upstream connection failure.
    pub fn upstream_error(reason: &str) -> Self {
        Self::with_code(
            format!("Failed to connect to model server: {reason}"),
            "server_error",
            "upstream_error",
        )
    }
}

impl From<ModelRuntimeError> for ErrorResponse {
    fn from(err: ModelRuntimeError) -> Self {
        match err {
            ModelRuntimeError::ModelNotFound(name) => Self::model_not_found(&name),
            ModelRuntimeError::ModelLoading => Self::model_loading(),
            ModelRuntimeError::SpawnFailed(reason) => Self::upstream_error(&reason),
            ModelRuntimeError::HealthCheckFailed(reason) => Self::upstream_error(&reason),
            ModelRuntimeError::ModelFileNotFound(path) => Self::with_code(
                format!("Model file not found: {path}"),
                "invalid_request_error",
                "model_file_not_found",
            ),
            ModelRuntimeError::Internal(msg) => Self::new(msg, "server_error"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // =========================================================================
    // ModelsResponse tests
    // =========================================================================

    #[test]
    fn models_response_from_empty_summaries() {
        let resp = ModelsResponse::from_summaries(vec![]);
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
                param_count: "8B".into(),
                quantization: Some("Q4_K_M".into()),
                architecture: Some("llama".into()),
                created_at: 1700000000,
                file_size: 4_000_000_000,
            },
            ModelSummary {
                id: 2,
                name: "mistral-7b-q8".into(),
                tags: vec![],
                param_count: "7B".into(),
                quantization: Some("Q8_0".into()),
                architecture: Some("mistral".into()),
                created_at: 1700000001,
                file_size: 7_000_000_000,
            },
        ];

        let resp = ModelsResponse::from_summaries(summaries);
        assert_eq!(resp.data.len(), 2);
        assert_eq!(resp.data[0].id, "llama-3-8b-q4");
        assert_eq!(resp.data[0].object, "model");
        assert_eq!(resp.data[0].owned_by, "gglib");
        assert_eq!(resp.data[0].created, 1700000000);
        assert!(resp.data[0].description.is_some());
    }

    #[test]
    fn models_response_serializes_to_openai_format() {
        let resp = ModelsResponse::from_summaries(vec![ModelSummary {
            id: 1,
            name: "test-model".into(),
            tags: vec![],
            param_count: "7B".into(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
        }]);

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
            param_count: "13B".into(),
            quantization: Some("Q5_K_S".into()),
            architecture: Some("llama".into()),
            created_at: 0,
            file_size: 0,
        };
        let info = ModelInfo::from(summary);
        let desc = info.description.unwrap();
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
            param_count: "1B".into(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
        };
        let info = ModelInfo::from(summary);
        let desc = info.description.unwrap();
        assert!(
            desc.contains("unknown"),
            "missing fields should show 'unknown'"
        );
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
        assert_eq!(
            req.stop,
            Some(StopSequences::Multiple(vec!["END".to_string()]))
        );
        assert_eq!(req.num_ctx, Some(8192));
        assert_eq!(req.tools.as_ref().unwrap().len(), 1);
        assert_eq!(req.tools.as_ref().unwrap()[0].function.name, "get_weather");
    }

    #[test]
    fn chat_request_accepts_stop_as_bare_string() {
        let json = r#"{
            "model": "llama-3",
            "messages": [{"role": "user", "content": "hi"}],
            "stop": "END"
        }"#;
        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.stop, Some(StopSequences::Single("END".to_string())));
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
}
