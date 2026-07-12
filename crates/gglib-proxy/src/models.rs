//! OpenAI API data models for request/response handling.
//!
//! This module contains types that match the OpenAI API specification.
//! Domain types live in `gglib-core`; this module handles the API layer mapping.

use gglib_core::ports::{ModelRuntimeError, ModelSummary};
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size};
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
    /// Stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
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
    ///
    /// Each model's `context_window` is resolved through the canonical
    /// [`resolve_context_size`] fallback chain (per-model server_defaults →
    /// global default), then clamped to the GGUF metadata ceiling so we never
    /// advertise more context than the model file supports.
    pub fn from_summaries(summaries: Vec<ModelSummary>, global_default_ctx: u64) -> Self {
        let data: Vec<ModelInfo> = summaries
            .into_iter()
            .map(|summary| {
                let effective_cap = resolve_context_size(&ServerConfigOptions {
                    context_size: None,
                    model_server_ctx: summary
                        .server_defaults
                        .as_ref()
                        .and_then(|sd| sd.context_length),
                    global_default_ctx: Some(global_default_ctx),
                    ..Default::default()
                });
                ModelInfo {
                    id: summary.name.clone(),
                    object: "model".to_string(),
                    created: summary.created_at,
                    owned_by: "gglib".to_string(),
                    description: Some(summary.description()),
                    context_window: summary.context_length.map(|ctx| ctx.min(effective_cap)),
                }
            })
            .collect();
        Self {
            object: "list".to_string(),
            data,
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
    /// Model's context window size, in tokens (llama.cpp's `/v1/models`
    /// field-naming convention). `None` when unknown.
    ///
    /// Populated from [`ModelSummary::context_length`] by default. The
    /// `/v1/models` handler (`server::list_models`) then adjusts it to the
    /// context the model would actually be served with: non-running models
    /// are clamped to the proxy's `default_ctx` (what `ensure_model_running`
    /// would launch them with), and the currently running model is
    /// overwritten with its live `effective_ctx` from
    /// `ModelRuntimePort::current_model()`. Clients that auto-detect context
    /// size from this endpoint (e.g. the GitHub Copilot LLM Gateway
    /// extension) read it once at picker-build time — usually before any
    /// model runs — so the pre-launch value must already be honest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
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

    /// Create an error response for startup contention timeout.
    ///
    /// Wire-format identical to `model_loading()` (`service_unavailable` type)
    /// so clients treat both as retryable with the same backoff behavior.
    pub fn contention_timeout(msg: &str) -> Self {
        Self::with_code(msg, "service_unavailable", "contention_timeout")
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

    /// Create an error response for context length exceeded.
    ///
    /// Returned as HTTP 400 when the proxy cannot reduce the history payload
    /// to within the safe budget after aggressive truncation.  The client
    /// should start a new conversation to clear history.
    pub fn context_length_exceeded() -> Self {
        Self::with_code(
            "Context window limit reached. Please start a new conversation.",
            "context_length_exceeded",
            "context_length_exceeded",
        )
    }

    /// Create an error response for a malformed or invalid request.
    pub fn invalid_request(msg: &str) -> Self {
        Self::with_code(msg, "invalid_request_error", "invalid_request")
    }

    /// Create an error response for an internal server error.
    pub fn internal_error(msg: &str) -> Self {
        Self::with_code(msg, "server_error", "internal_error")
    }
}

impl From<ModelRuntimeError> for ErrorResponse {
    fn from(err: ModelRuntimeError) -> Self {
        match err {
            ModelRuntimeError::ModelNotFound(name) => Self::model_not_found(&name),
            ModelRuntimeError::ModelLoading => Self::model_loading(),
            ModelRuntimeError::SpawnFailed(reason) => Self::upstream_error(&reason),
            ModelRuntimeError::HealthCheckFailed(reason) => Self::upstream_error(&reason),
            ModelRuntimeError::ContentionTimeout(msg) => Self::contention_timeout(&msg),
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
#[path = "models_tests.rs"]
mod tests;
