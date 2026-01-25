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
pub struct CreateConversationRequest {
    pub title: Option<String>,
    pub model_id: Option<i64>,
    pub system_prompt: Option<String>,
}

/// Request body for updating a conversation.
#[derive(Debug, Deserialize)]
pub struct UpdateConversationRequest {
    pub title: Option<String>,
    pub system_prompt: Option<Option<String>>,
}

/// Request body for saving a new message.
#[derive(Debug, Deserialize)]
pub struct SaveMessageRequest {
    pub conversation_id: i64,
    pub role: String,
    pub content: String,
}

/// Request body for updating a message.
#[derive(Debug, Deserialize)]
pub struct UpdateMessageRequest {
    pub content: String,
}

/// Request body for chat completion proxy.
#[derive(Debug, Deserialize)]
pub struct ChatProxyRequest {
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
    /// Optional max tokens.
    pub max_tokens: Option<u32>,
    /// Optional temperature.
    pub temperature: Option<f32>,
    /// Optional tools for function calling.
    #[serde(default)]
    pub tools: Option<Vec<serde_json::Value>>,
    /// Optional tool choice strategy.
    #[serde(default)]
    pub tool_choice: Option<serde_json::Value>,
}

/// A chat message in the request/response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    /// Content is optional when tool_calls are present (OpenAI API spec)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool call ID (for tool role messages returning results).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool calls made by the assistant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
}

/// Response from llama-server chat completion (non-streaming).
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ChatUsage>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatUsage {
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
/// This router does NOT include CORS middleware. Add it at the call site:
///
/// ```ignore
/// let router = chat_routes(state).layer(cors_layer);
/// ```
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
pub async fn list_conversations(
    State(state): State<AppState>,
) -> Result<Json<Vec<Conversation>>, HttpError> {
    let conversations = state.core.chat_history().list_conversations().await?;
    Ok(Json(conversations))
}

/// Create a new conversation.
/// POST /api/conversations
pub async fn create_conversation(
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
pub async fn get_conversation(
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
pub async fn update_conversation(
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
pub async fn delete_conversation(
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
pub async fn get_messages(
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
pub async fn save_message(
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
        })
        .await?;
    Ok(Json(id))
}

/// Update a message's content.
/// PUT /api/messages/:id
pub async fn update_message(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateMessageRequest>,
) -> Result<(), HttpError> {
    state
        .core
        .chat_history()
        .update_message(id, req.content)
        .await?;
    Ok(())
}

/// Delete a message and all subsequent messages in the conversation.
/// DELETE /api/messages/:id
pub async fn delete_message(
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

/// Allowed port range for llama-server connections.
/// Prevents the endpoint from becoming a generic SSRF dialer.
const MIN_ALLOWED_PORT: u16 = 1024;
const MAX_ALLOWED_PORT: u16 = 65535;

/// Validate the requested port is within allowed range and corresponds
/// to a running server.
async fn validate_port(state: &AppState, port: u16) -> Result<(), HttpError> {
    // Basic range check
    if !(MIN_ALLOWED_PORT..=MAX_ALLOWED_PORT).contains(&port) {
        return Err(HttpError::BadRequest(format!(
            "Port {} is outside allowed range ({}-{})",
            port, MIN_ALLOWED_PORT, MAX_ALLOWED_PORT
        )));
    }

    // Check if the port matches a running server
    let servers = state.gui.list_servers().await;
    let port_in_use = servers.iter().any(|s| s.port == port);

    if !port_in_use {
        return Err(HttpError::BadRequest(format!(
            "No running server found on port {}. Start a server first.",
            port
        )));
    }

    Ok(())
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
pub async fn proxy_chat(
    State(state): State<AppState>,
    Json(request): Json<ChatProxyRequest>,
) -> Result<Response, HttpError> {
    // Validate the port
    validate_port(&state, request.port).await?;

    // Look up the model by port to determine capabilities
    let servers = state.gui.list_servers().await;
    let server = servers.iter().find(|s| s.port == request.port);

    let capabilities = if let Some(server) = server {
        // Found the server, fetch the model to get its capabilities
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
                model.capabilities
            }
            Ok(None) => {
                tracing::warn!(
                    port = request.port,
                    model_id = server.model_id,
                    "Model not found for capability detection; assuming default"
                );
                gglib_core::domain::ModelCapabilities::default()
            }
            Err(e) => {
                tracing::warn!(
                    port = request.port,
                    model_id = server.model_id,
                    error = %e,
                    "Failed to fetch model for capability detection; assuming default"
                );
                gglib_core::domain::ModelCapabilities::default()
            }
        }
    } else {
        tracing::warn!(
            port = request.port,
            "No server found for port; assuming default capabilities"
        );
        gglib_core::domain::ModelCapabilities::default()
    };

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

    // Convert to ChatMessage format and apply capability-aware transformations
    let core_messages: Vec<gglib_core::ChatMessage> = valid_messages
        .into_iter()
        .map(|m| gglib_core::ChatMessage {
            role: m.role,
            content: m.content,
            tool_calls: m.tool_calls.map(serde_json::Value::Array),
        })
        .collect();

    let transformed = gglib_core::transform_messages_for_capabilities(core_messages, capabilities);

    // Convert back to ChatMessage
    let final_messages: Vec<ChatMessage> = transformed
        .into_iter()
        .map(|m| ChatMessage {
            role: m.role,
            content: m.content,
            tool_calls: m.tool_calls.and_then(|v| {
                if let serde_json::Value::Array(arr) = v {
                    Some(arr)
                } else {
                    None
                }
            }),
            tool_call_id: None,
        })
        .collect();

    // Build the llama-server URL
    let server_url = format!("http://127.0.0.1:{}/v1/chat/completions", request.port);

    // Build the forwarded request body
    let mut forward_body = serde_json::json!({
        "model": request.model,
        "messages": final_messages,
        "stream": request.stream,
        "max_tokens": request.max_tokens,
        "temperature": request.temperature,
    });

    // Add tools if provided
    if let Some(tools) = &request.tools
        && !tools.is_empty()
    {
        forward_body["tools"] = serde_json::json!(tools);
    }
    if let Some(tool_choice) = &request.tool_choice {
        forward_body["tool_choice"] = tool_choice.clone();
    }

    // DEBUG: Log the exact payload sent to llama-server
    let log_path = std::env::var("HOME")
        .map(|h| format!("{}/llama-request-debug.json", h))
        .unwrap_or_else(|_| "/tmp/llama-request-debug.json".to_string());

    if let Ok(json_str) = serde_json::to_string_pretty(&forward_body) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let log_entry = format!(
            "\n=== REQUEST {} ===\npath: /api/chat (chat_api::proxy_chat)\n{}\n====================================\n",
            timestamp, json_str
        );
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, log_entry.as_bytes()));
    }

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
