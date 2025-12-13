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

/// Request to /v1/chat/completions endpoint.
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
    #[serde(skip_serializing_if = "Option::is_none")]
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
