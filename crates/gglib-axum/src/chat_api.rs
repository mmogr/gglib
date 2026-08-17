//! Chat API routes and handlers.
//!
//! This module provides chat-related endpoints for conversation management
//! and chat completion proxying to llama-server instances.
//!
//! Chat handlers use the unified `AppState` from `routes.rs` and access
//! `core` and `gui` services through it.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::HttpError;
use crate::state::AppState;
use gglib_core::domain::chat::{Conversation, Message, MessageRole, NewMessage};

// ─────────────────────────────────────────────────────────────────────────────
// Request/Response DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Request body for creating a new conversation.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct CreateConversationRequest {
    pub title: Option<String>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub model_id: Option<i64>,
    pub system_prompt: Option<String>,
}

/// Request body for updating a conversation.
///
/// `system_prompt` uses `serde_with::rust::double_option` so an explicit
/// JSON `null` (clear the system prompt) is distinguished from an omitted
/// key (leave unchanged) — without it, `PUT /api/conversations/:id` with
/// `{"system_prompt": null}` silently no-ops instead of clearing the prompt.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct UpdateConversationRequest {
    pub title: Option<String>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "string | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub system_prompt: Option<Option<String>>,
}

/// Request body for saving a new message.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct SaveMessageRequest {
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub conversation_id: i64,
    pub role: String,
    pub content: String,
    /// Opaque to gglib — stored and handed back verbatim. `unknown` rather
    /// than a modelled shape because that is exactly what the server promises.
    #[cfg_attr(feature = "ts-bindings", ts(type = "unknown | null"))]
    pub metadata: Option<serde_json::Value>,
}

/// Request body for updating a message.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct UpdateMessageRequest {
    pub content: String,
    #[cfg_attr(feature = "ts-bindings", ts(type = "unknown | null"))]
    pub metadata: Option<serde_json::Value>,
}

/// Request body for chat completion proxy.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatProxyRequest {
    /// The port of the llama-server to forward to.
    pub port: u16,
    /// The model identifier (not used for routing, just forwarded).
    #[serde(default)]
    pub model: String,
    /// The messages to send.
    pub messages: Vec<ChatMessage>,
    /// Whether to stream the response.
    #[serde(default)]
    pub stream: bool,
    /// Optional max tokens (inference parameter - will be resolved via hierarchy).
    pub max_tokens: Option<u32>,
    /// Optional temperature (inference parameter - will be resolved via hierarchy).
    pub temperature: Option<f32>,
    /// Optional top_p (inference parameter - will be resolved via hierarchy).
    pub top_p: Option<f32>,
    /// Optional top_k (inference parameter - will be resolved via hierarchy).
    pub top_k: Option<i32>,
    /// Optional repeat_penalty (inference parameter - will be resolved via hierarchy).
    pub repeat_penalty: Option<f32>,
    /// Optional presence_penalty (inference parameter - will be resolved via hierarchy).
    pub presence_penalty: Option<f32>,
    /// Optional min_p sampling threshold (inference parameter - will be resolved via hierarchy).
    pub min_p: Option<f32>,
    /// How hard to ask the model to think, where its chat template reads the
    /// variable (inference parameter - will be resolved via hierarchy).
    ///
    /// Conditional by nature: a template that does not read `reasoning_effort`
    /// ignores it in silence, and gglib deletes the key rather than sending it
    /// on a model whose observed caps say so (ADR 0007 decision 3). A client
    /// that wants a promise the template cannot break wants
    /// [`Self::reasoning_budget_tokens`], which llama.cpp enforces sampler-side.
    ///
    /// No `none` level is offered here or anywhere else in gglib: erasing the
    /// kwarg yields the template's own default, which is a different act from
    /// naming a level, and is what omitting this field already does.
    pub reasoning_effort: Option<gglib_core::domain::ReasoningEffort>,
    /// Ceiling on this turn's thinking tokens (inference parameter - will be
    /// resolved via hierarchy).
    ///
    /// `-1` defers to the launch-time default; `0` stops thinking altogether.
    /// Upstream range-validates it, so a value gglib forwards and llama-server
    /// dislikes comes back as an honest HTTP 400 naming the range.
    pub reasoning_budget_tokens: Option<i32>,
    /// Optional tools for function calling.
    ///
    /// Forwarded to llama-server untouched, so gglib models no shape here and
    /// TypeScript says so: `unknown[]`, not a guessed `Tool` interface.
    #[cfg_attr(feature = "ts-bindings", ts(type = "unknown[] | null"))]
    #[serde(default)]
    pub tools: Option<Vec<serde_json::Value>>,
    /// Optional tool choice strategy.
    #[cfg_attr(feature = "ts-bindings", ts(type = "unknown | null"))]
    #[serde(default)]
    pub tool_choice: Option<serde_json::Value>,
}

/// A chat message in the request/response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct ChatMessage {
    pub role: String,
    /// Content is optional when tool_calls are present (OpenAI API spec)
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool call ID (for tool role messages returning results).
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool calls made by the assistant.
    #[cfg_attr(feature = "ts-bindings", ts(type = "unknown[]", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
}

/// Response from llama-server chat completion (non-streaming).
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ChatUsage>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct ChatUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Router Factory
// ─────────────────────────────────────────────────────────────────────────────

/// Create a router with chat-only API endpoints.
///
/// This router provides:
/// - `/api/conversations` - List/create conversations
/// - `/api/conversations/{id}` - Get/update/delete conversation
/// - `/api/conversations/{id}/messages` - Get messages for conversation
/// - `/api/messages` - Save new message
/// - `/api/messages/{id}` - Update/delete message
/// - `/api/chat` - Proxy chat completions to llama-server (streaming supported)
///
/// # Arguments
///
/// * `state` - Shared chat API context
///
/// # Returns
///
/// An Axum router with all chat endpoints configured.
///
/// # Note
///
/// This router does NOT include CORS middleware. Apply it at the call site
/// before merging into the main router.
///
/// Build chat routes without `/api` prefix for nesting under /api.
///
/// Returns a router typed as `Router<AppState>` (state inferred from handlers)
/// but WITHOUT `.with_state()` applied. The caller must apply `.with_state()` before
/// nesting. All routes use handlers that expect `State<AppState>`.
pub(crate) fn chat_routes_no_prefix() -> Router<AppState> {
    Router::new()
        // Conversation endpoints (no /api prefix - will be nested)
        .route(
            "/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route(
            "/conversations/{id}",
            get(get_conversation)
                .put(update_conversation)
                .delete(delete_conversation),
        )
        // Message endpoints
        .route("/conversations/{id}/messages", get(get_messages))
        .route("/messages", post(save_message))
        .route("/messages/{id}", put(update_message).delete(delete_message))
        // Chat completion proxy (forwards to llama-server)
        .route("/chat", post(proxy_chat))
}

// ─────────────────────────────────────────────────────────────────────────────
// Conversation Handlers
// ─────────────────────────────────────────────────────────────────────────────

/// List all conversations.
/// GET /api/conversations
pub(crate) async fn list_conversations(
    State(state): State<AppState>,
) -> Result<Json<Vec<Conversation>>, HttpError> {
    let conversations = state.core.chat_history().list_conversations().await?;
    Ok(Json(conversations))
}

/// Create a new conversation.
/// POST /api/conversations
pub(crate) async fn create_conversation(
    State(state): State<AppState>,
    Json(req): Json<CreateConversationRequest>,
) -> Result<Json<i64>, HttpError> {
    let title = req.title.unwrap_or_else(|| "New Conversation".to_string());
    let id = state
        .core
        .chat_history()
        .create_conversation(title, req.model_id, req.system_prompt)
        .await?;
    Ok(Json(id))
}

/// Get a single conversation by ID.
/// GET /api/conversations/:id
pub(crate) async fn get_conversation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Conversation>, HttpError> {
    let conversation = state
        .core
        .chat_history()
        .get_conversation(id)
        .await?
        .ok_or_else(|| HttpError::NotFound(format!("Conversation not found: {}", id)))?;
    Ok(Json(conversation))
}

/// Update a conversation.
/// PUT /api/conversations/:id
pub(crate) async fn update_conversation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateConversationRequest>,
) -> Result<(), HttpError> {
    state
        .core
        .chat_history()
        .update_conversation(id, req.title, req.system_prompt)
        .await?;
    Ok(())
}

/// Delete a conversation and all its messages.
/// DELETE /api/conversations/:id
pub(crate) async fn delete_conversation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(), HttpError> {
    state.core.chat_history().delete_conversation(id).await?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Message Handlers
// ─────────────────────────────────────────────────────────────────────────────

/// Get all messages for a conversation.
/// GET /api/conversations/:id/messages
pub(crate) async fn get_messages(
    State(state): State<AppState>,
    Path(conversation_id): Path<i64>,
) -> Result<Json<Vec<Message>>, HttpError> {
    let messages = state
        .core
        .chat_history()
        .get_messages(conversation_id)
        .await?;
    Ok(Json(messages))
}

/// Save a new message.
/// POST /api/messages
pub(crate) async fn save_message(
    State(state): State<AppState>,
    Json(req): Json<SaveMessageRequest>,
) -> Result<Json<i64>, HttpError> {
    let role = MessageRole::parse(&req.role)
        .ok_or_else(|| HttpError::BadRequest(format!("Invalid message role: {}", req.role)))?;

    let id = state
        .core
        .chat_history()
        .save_message(NewMessage {
            conversation_id: req.conversation_id,
            role,
            content: req.content,
            metadata: req.metadata,
        })
        .await?;
    Ok(Json(id))
}

/// Update a message's content.
/// PUT /api/messages/:id
pub(crate) async fn update_message(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateMessageRequest>,
) -> Result<(), HttpError> {
    state
        .core
        .chat_history()
        .update_message(id, req.content, req.metadata)
        .await?;
    Ok(())
}

/// Delete a message and all subsequent messages in the conversation.
/// DELETE /api/messages/:id
pub(crate) async fn delete_message(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<i64>, HttpError> {
    let deleted_count = state
        .core
        .chat_history()
        .delete_message_and_subsequent(id)
        .await?;
    Ok(Json(deleted_count))
}

// ─────────────────────────────────────────────────────────────────────────────
// Chat Proxy Handler
// ─────────────────────────────────────────────────────────────────────────────

use crate::handlers::port_utils::validate_port;

/// Inject tools and tool_choice into the forwarded request body, gated on
/// whether the model advertises `SUPPORTS_TOOL_CALLS`.
///
/// Takes `tools` and `tool_choice` as individual field references rather than
/// the whole `ChatProxyRequest` because `request.messages` is consumed earlier
/// in `proxy_chat` via `into_iter()`, leaving the struct partially moved.
///
/// When the capability flag is absent **and** the request actually contained
/// tools or a tool_choice, a debug trace is emitted so operators can verify
/// the strip behaviour without flooding logs on ordinary non-tool requests.
fn apply_tools_to_body(
    body: &mut serde_json::Value,
    tools: &Option<Vec<serde_json::Value>>,
    tool_choice: &Option<serde_json::Value>,
    capabilities: gglib_core::domain::ModelCapabilities,
) {
    if capabilities.contains(gglib_core::domain::ModelCapabilities::SUPPORTS_TOOL_CALLS) {
        if let Some(tools) = tools
            && !tools.is_empty()
        {
            body["tools"] = serde_json::json!(tools);
        }
        if let Some(tc) = tool_choice {
            body["tool_choice"] = serde_json::json!(tc);
        }
    } else {
        let has_tools = tools.as_ref().is_some_and(|t| !t.is_empty());
        let has_tool_choice = tool_choice.is_some();
        if has_tools || has_tool_choice {
            tracing::debug!("Stripping tools from request — model does not support tool calling");
        }
    }
}

/// Proxy chat completion requests to a running llama-server.
///
/// POST /api/chat
///
/// This handler forwards chat completion requests to the specified llama-server
/// instance and returns the response. Supports both streaming (SSE) and
/// non-streaming (JSON) modes.
///
/// # Security
///
/// - Port must be within allowed range (1024-65535)
/// - Port must correspond to a currently running server
pub(crate) async fn proxy_chat(
    State(state): State<AppState>,
    Json(request): Json<ChatProxyRequest>,
) -> Result<Response, HttpError> {
    // Validate the port
    validate_port(&state, request.port).await?;

    // Look up the model by port to determine capabilities
    let servers = state.servers.list_servers().await;
    let server = servers.iter().find(|s| s.port == request.port);

    let (capabilities, model_defaults, model_ctx) = if let Some(server) = server {
        // Found the server, fetch the model to get its capabilities and inference_defaults
        match state.core.models().get_by_id(server.model_id).await {
            Ok(Some(model)) => {
                tracing::debug!(
                    port = request.port,
                    model_id = server.model_id,
                    model_name = %model.name,
                    capabilities = model.capabilities.bits(),
                    supports_system = model.capabilities.contains(gglib_core::domain::ModelCapabilities::SUPPORTS_SYSTEM_ROLE),
                    requires_strict_turns = model.capabilities.contains(gglib_core::domain::ModelCapabilities::REQUIRES_STRICT_TURNS),
                    "Model capabilities loaded for chat request"
                );
                let model_ctx = gglib_core::domain::ModelSamplingContext::for_model(&model);
                (model.capabilities, model.inference_defaults, model_ctx)
            }
            Ok(None) => {
                tracing::warn!(
                    port = request.port,
                    model_id = server.model_id,
                    "Model not found for capability detection; assuming default"
                );
                (
                    gglib_core::domain::ModelCapabilities::default(),
                    None,
                    gglib_core::domain::ModelSamplingContext::default(),
                )
            }
            Err(e) => {
                tracing::warn!(
                    port = request.port,
                    model_id = server.model_id,
                    error = %e,
                    "Failed to fetch model for capability detection; assuming default"
                );
                (
                    gglib_core::domain::ModelCapabilities::default(),
                    None,
                    gglib_core::domain::ModelSamplingContext::default(),
                )
            }
        }
    } else {
        tracing::warn!(
            port = request.port,
            "No server found for port; assuming default capabilities"
        );
        (
            gglib_core::domain::ModelCapabilities::default(),
            None,
            gglib_core::domain::ModelSamplingContext::default(),
        )
    };

    // Load global settings for inference defaults
    let global_defaults = state
        .core
        .settings()
        .get()
        .await
        .ok()
        .and_then(|s| s.inference_defaults);

    // Resolve inference parameters using the 4-level hierarchy:
    // Request → Model → Global → Hardcoded defaults
    let resolved = gglib_core::domain::InferenceConfig {
        temperature: request.temperature,
        top_p: request.top_p,
        top_k: request.top_k,
        max_tokens: request.max_tokens,
        repeat_penalty: request.repeat_penalty,
        presence_penalty: request.presence_penalty,
        min_p: request.min_p,
        // The WebUI chat request offers no DRY, entropy-adaptive or
        // frequency-penalty controls, so the request layer names none. The
        // per-model, global and floor layers below still resolve in normally —
        // this is an absent opinion, not a disabled feature.
        dry_multiplier: None,
        dry_base: None,
        dry_allowed_length: None,
        dry_penalty_last_n: None,
        dynatemp_range: None,
        dynatemp_exponent: None,
        top_n_sigma: None,
        frequency_penalty: None,
        // The WebUI chat is interactive, where a pinned seed would make every
        // regeneration return the identical text. Reproducibility is a
        // benchmark's need, not a chat's.
        seed: None,
        // Both halves of the reasoning pair, taken from the request. The
        // trust split that governs them in the proxy does not apply here:
        // `/api/chat` is gglib's own local UI endpoint and resolves the
        // hierarchy by hand rather than through
        // `request_pipeline::resolve_sampling`, so every field above — from
        // `temperature` down — is already accepted from the caller unexamined.
        // Adding a gate for these two alone would describe a boundary this
        // endpoint does not have.
        //
        // The suppression gate does **not** run here, and that is worth stating
        // rather than assuming. This handler posts straight to
        // `127.0.0.1:{port}/v1/chat/completions` — llama-server itself, not the
        // embedded proxy — so no stage of `request_pipeline::apply` executes on
        // this path, stage 5b included. A level named here reaches a template
        // that may ignore it in silence.
        //
        // That is why `ModelDetailDto` now carries the template's
        // `reasoning_effort` support: the gating this path cannot do server-side
        // is done by the client that decides whether to offer the control at
        // all. Moving this endpoint onto the pipeline is the real fix and is
        // a change of its own — every other stage is missing here too.
        reasoning_effort: request.reasoning_effort,
        reasoning_budget_tokens: request.reasoning_budget_tokens,
    }
    .resolve_with_defaults(model_defaults.as_ref(), global_defaults.as_ref(), model_ctx);

    tracing::debug!(
        port = request.port,
        resolved_temperature = resolved.temperature,
        resolved_top_p = resolved.top_p,
        resolved_top_k = resolved.top_k,
        resolved_max_tokens = resolved.max_tokens,
        resolved_repeat_penalty = resolved.repeat_penalty,
        resolved_presence_penalty = resolved.presence_penalty,
        resolved_min_p = resolved.min_p,
        resolved_dry_multiplier = resolved.dry_multiplier,
        resolved_dry_base = resolved.dry_base,
        resolved_dry_allowed_length = resolved.dry_allowed_length,
        resolved_dry_penalty_last_n = resolved.dry_penalty_last_n,
        // Logged for a reason the others do not have: neither reasoning control
        // is echoed by `/slots` or `/props` (ADR 0007 finding 7a), so on this
        // path — which has no provenance record and no readback — this line is
        // the only evidence either was resolved.
        resolved_reasoning_effort = ?resolved.reasoning_effort,
        resolved_reasoning_budget_tokens = resolved.reasoning_budget_tokens,
        "Resolved inference parameters via hierarchy"
    );

    // Filter out messages with empty or whitespace-only content
    // EXCEPT: tool role messages (they return results) and assistant messages with tool_calls
    // This prevents Jinja template errors in llama-server
    let valid_messages: Vec<_> = request
        .messages
        .into_iter()
        .filter(|m| {
            // Keep if content is non-empty
            if let Some(content) = &m.content
                && !content.trim().is_empty()
            {
                return true;
            }
            // Keep tool messages and messages with tool_calls even if content is empty/null
            m.role == "tool" || m.tool_calls.is_some()
        })
        .collect();

    if valid_messages.is_empty() {
        return Err(HttpError::BadRequest(
            "No valid messages to send. All messages have empty content.".into(),
        ));
    }

    // Convert to ChatMessage format and apply capability-aware transformations.
    //
    // `tool_call_id` travels in the core type's catch-all rather than being
    // dropped and re-synthesized: it is required by the chat templates of the
    // strict-turn models this transform exists for, so losing it here would
    // break tool calling on exactly those models.
    let core_messages: Vec<gglib_core::ChatMessage> = valid_messages
        .into_iter()
        .map(|m| {
            let mut extra = serde_json::Map::new();
            if let Some(id) = m.tool_call_id {
                extra.insert("tool_call_id".to_owned(), serde_json::Value::String(id));
            }
            gglib_core::ChatMessage {
                role: m.role,
                content: m.content.map(gglib_core::MessageContent::Text),
                tool_calls: m.tool_calls.map(serde_json::Value::Array),
                extra,
            }
        })
        .collect();

    let transformed = gglib_core::transform_messages_for_capabilities(core_messages, capabilities);

    // Convert back to ChatMessage
    let final_messages: Vec<ChatMessage> = transformed
        .into_iter()
        .map(|mut m| ChatMessage {
            role: m.role,
            content: m.content.map(|c| c.into_string()),
            tool_calls: m.tool_calls.and_then(|v| {
                if let serde_json::Value::Array(arr) = v {
                    Some(arr)
                } else {
                    None
                }
            }),
            tool_call_id: m
                .extra
                .remove("tool_call_id")
                .and_then(|v| v.as_str().map(str::to_owned)),
        })
        .collect();

    // Build the llama-server URL
    let server_url = format!("http://127.0.0.1:{}/v1/chat/completions", request.port);

    // Build the forwarded request body: transport fields here, sampling from
    // the shared patch helper.
    //
    // Listing the sampling keys by hand emitted `"key": null` for every unset
    // parameter — `max_tokens` is unset by design, so this path was already
    // sending a null llama-server has no business receiving. It also meant a
    // parameter added to `InferenceConfig` silently never reached this
    // surface. `to_openai_json_patch` filters `None` and covers every field,
    // present and future, which is why the other two request paths use it.
    let mut forward_body = serde_json::json!({
        "model": request.model,
        "messages": final_messages,
        "stream": request.stream,
    });
    if let Some(obj) = forward_body.as_object_mut() {
        for (key, value) in resolved.to_openai_json_patch() {
            obj.insert(key, value);
        }
    }

    // Inject tools only when the model supports them.
    // Note: request.messages was consumed above, so we pass fields individually.
    apply_tools_to_body(
        &mut forward_body,
        &request.tools,
        &request.tool_choice,
        capabilities,
    );

    // Forward the request
    let client = Client::new();
    let response = client
        .post(&server_url)
        .header("Content-Type", "application/json")
        .json(&forward_body)
        .send()
        .await
        .map_err(|e| {
            HttpError::ServiceUnavailable(format!(
                "Failed to connect to llama-server on port {}: {}",
                request.port, e
            ))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(HttpError::Internal(format!(
            "llama-server returned {}: {}",
            status, error_text
        )));
    }

    if request.stream {
        // Streaming mode: pass through SSE stream unchanged
        let stream = response
            .bytes_stream()
            .map(|result| result.map_err(std::io::Error::other));

        let body = Body::from_stream(stream);

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header(header::CONNECTION, "keep-alive")
            .body(body)
            .unwrap()
            .into_response())
    } else {
        // Non-streaming mode: parse and return JSON
        let completion: ChatCompletionResponse = response.json().await.map_err(|e| {
            HttpError::Internal(format!("Failed to parse llama-server response: {}", e))
        })?;

        Ok(Json(completion).into_response())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "chat_api_tests.rs"]
mod chat_api_tests;
