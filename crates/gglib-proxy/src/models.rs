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

/// The one field the proxy reads out of a `/v1/embeddings` request body.
///
/// Same principle as [`ChatRoutingEnvelope`]: `input` is deliberately not
/// declared. Both OpenAI shapes (a bare string and an array of strings), plus
/// `encoding_format`, `dimensions` and anything llama-server grows later, ride
/// through as raw bytes because nothing here ever looks at them.
#[derive(Debug, Deserialize)]
pub(crate) struct EmbeddingsRoutingEnvelope {
    /// Model name or ID used to select the llama-server instance.
    pub model: String,
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
                    capabilities: capabilities_of(&summary),
                }
            })
            .collect();
        Self {
            object: "list".to_string(),
            data,
        }
    }
}

/// The extra endpoints a catalogued model can serve, for
/// [`ModelInfo::capabilities`].
///
/// `None` rather than an empty vec for an ordinary chat model, so the field
/// disappears from the response instead of appearing as `[]` — an empty list
/// reads as "this model can do nothing", which is the opposite of the truth.
fn capabilities_of(summary: &ModelSummary) -> Option<Vec<String>> {
    summary
        .tags
        .iter()
        .any(|t| t == crate::embeddings::EMBEDDING_TAG)
        .then(|| vec!["embeddings".to_string()])
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
    /// are clamped to the proxy's `default_ctx` (what `admit`
    /// would launch them with), and the currently running model is
    /// overwritten with its live `effective_ctx` from
    /// `ModelRuntimePort::current_model()`. Clients that auto-detect context
    /// size from this endpoint (e.g. the GitHub Copilot LLM Gateway
    /// extension) read it once at picker-build time — usually before any
    /// model runs — so the pre-launch value must already be honest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    /// Non-OpenAI endpoints this model can serve, beyond
    /// `/v1/chat/completions`.
    ///
    /// `Some(["embeddings"])` for a model tagged `embedding`; `None` — and so
    /// absent from the JSON entirely — for everything else. A chat client's
    /// picker is therefore byte-identical to what it saw before this field
    /// existed, while a RAG client has something to filter on other than
    /// guessing from the model's name.
    ///
    /// An array rather than a `type` discriminant because capability is not
    /// exclusive: a future entry may serve both chat and embeddings, and
    /// vision or tool support could join the same list without a second field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
}

// =============================================================================
// Error Response Types
// =============================================================================

/// Error response matching OpenAI format.
///
/// `Deserialize` is derived so in-process clients — the agent path's LLM
/// completion adapter, which talks to this proxy over loopback HTTP — can
/// classify a failure by reading the same struct the proxy wrote, rather than
/// re-deriving the contract from a second hand-written DTO that could drift.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

/// Error detail within an error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Create an error response for an admission-queue timeout.
    ///
    /// Wire-format identical to `model_loading()` (`service_unavailable` type)
    /// so clients treat both as retryable with the same backoff behavior. The
    /// `code` differs so a client that wants to can tell "this model is still
    /// starting" from "this endpoint is oversubscribed and never got to me".
    pub fn admission_timeout(msg: &str) -> Self {
        Self::with_code(msg, "service_unavailable", "admission_timeout")
    }

    /// Create an error response for model not found.
    pub fn model_not_found(model: &str) -> Self {
        Self::with_code(
            format!("Model '{model}' not found"),
            "invalid_request_error",
            "model_not_found",
        )
    }

    /// Create an error response for a model id whose `:{suffix}` matches no
    /// configured inference profile.
    ///
    /// The suffix is ambiguous by nature — it may be a profile that was
    /// renamed or deleted, or a model tag that was never in the catalog — so
    /// the message covers both readings instead of guessing, and lists the
    /// profiles that do exist. `available` is `None` when none are configured.
    pub fn profile_not_found(requested: &str, suffix: &str, available: Option<&str>) -> Self {
        let profiles = available.map_or_else(
            || "no inference profiles are configured".to_owned(),
            |names| format!("configured profiles are: {names}"),
        );
        Self::with_code(
            format!(
                "Model '{requested}' not found, and '{suffix}' is not an inference profile \
                 ({profiles})"
            ),
            "invalid_request_error",
            "profile_not_found",
        )
    }

    /// Create an error response for an embeddings request naming a model that
    /// is not an embedding model.
    ///
    /// Refused before the model is loaded rather than after: llama-server only
    /// answers `/v1/embeddings` when it was started with `--embeddings`, and
    /// gglib only passes that flag for a model tagged `embedding`. Forwarding
    /// anyway would evict whatever is currently serving chat to start a server
    /// that can only reply 501, so the swap is worth nothing to anyone.
    ///
    /// The remedy is named in the message because the failure is equally
    /// likely to be a client naming the wrong model and detection having
    /// missed a genuine embedding model.
    pub fn not_an_embedding_model(model: &str) -> Self {
        Self::with_code(
            format!(
                "Model '{model}' is not an embedding model. Only models tagged 'embedding' can \
                 serve /v1/embeddings. If this model does produce embeddings, run \
                 `gglib model retag {model}` to re-derive its tags."
            ),
            "invalid_request_error",
            "not_an_embedding_model",
        )
    }

    /// Create an error response for a chat request naming an embedding model.
    ///
    /// The mirror of [`Self::not_an_embedding_model`], and refused for the same
    /// reason: a model tagged `embedding` is launched with `--embeddings`,
    /// which makes that llama-server refuse chat completions. Swapping to it
    /// would unload whatever is currently serving chat in order to collect a
    /// 501.
    pub fn embedding_model_cannot_chat(model: &str) -> Self {
        Self::with_code(
            format!(
                "Model '{model}' is an embedding model and cannot serve chat completions. Use \
                 /v1/embeddings for it, or name a different model here."
            ),
            "invalid_request_error",
            "embedding_model_cannot_chat",
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

    /// Create an error response for a detected agentic tool-call loop.
    ///
    /// Returned as HTTP 400 by the pre-dispatch loop guard when the request's
    /// replayed history repeats the same tool-call batch beyond the shared
    /// agent-path threshold (see `loop_guard`).  `type` and `code` are both
    /// `loop_detected`, mirroring [`Self::context_length_exceeded`]'s shape.
    pub fn loop_detected(signature: &str) -> Self {
        Self::with_code(
            format!(
                "Agentic loop detected: this conversation repeats the same tool-call batch \
                 (signature: {signature}). Aborting before another identical turn. Start a new \
                 conversation or change approach; to disable this guard run \
                 `gglib config settings set --proxy-loop-detection false`."
            ),
            "loop_detected",
            "loop_detected",
        )
    }

    /// Create an error response for detected response stagnation.
    ///
    /// Returned as HTTP 400 by the pre-dispatch loop guard when the request's
    /// replayed history contains the same assistant response `count` times
    /// against a limit of `max_steps`.
    pub fn stagnation_detected(count: usize, max_steps: usize) -> Self {
        Self::with_code(
            format!(
                "Stagnation detected: the assistant has produced the same response {count} \
                 times (limit {max_steps}). Aborting before another identical turn. Start a \
                 new conversation or change approach; to disable this guard run \
                 `gglib config settings set --proxy-loop-detection false`."
            ),
            "stagnation_detected",
            "stagnation_detected",
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
            ModelRuntimeError::AdmissionTimeout(msg) => Self::admission_timeout(&msg),
            ModelRuntimeError::ModelFileNotFound(path) => Self::with_code(
                format!("Model file not found: {path}"),
                "invalid_request_error",
                "model_file_not_found",
            ),
            // Named separately from `model_not_found` so a client can tell
            // "no such model anywhere" from "this endpoint only serves one
            // model, and it isn't that one" — the latter is actionable by
            // changing the request, not the catalog.
            ModelRuntimeError::PinnedModelMismatch {
                expected,
                requested,
            } => Self::with_code(
                format!(
                    "This endpoint is pinned to model '{expected}' and cannot serve '{requested}'"
                ),
                "invalid_request_error",
                "pinned_model_mismatch",
            ),
            ModelRuntimeError::Internal(msg) => Self::new(msg, "server_error"),
        }
    }
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
