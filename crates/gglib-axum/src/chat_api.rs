//! Chat-only API router for embedded use cases.
//!
//! This module provides a minimal router with just chat-related endpoints,
//! suitable for embedding in Tauri or other contexts that don't need the
//! full AxumContext (downloads, HF client, process runner, etc.).
//!
//! This is the **single source of truth** for chat HTTP handlers. The full
//! Axum router in `routes.rs` merges these routes, and Tauri's embedded
//! server uses them directly.
//!
//! # Usage
//!
//! ```ignore
//! use gglib_axum::chat_api::{ChatApiContext, chat_routes};
//!
//! let state = Arc::new(ChatApiContext {
//!     core: app_core.clone(),
//!     gui: gui_backend.clone(),
//! });
//!
//! let router = chat_routes(state);
//! ```

use std::sync::Arc;

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
use gglib_core::domain::chat::{Conversation, Message, MessageRole, NewMessage};
use gglib_core::services::AppCore;
use gglib_gui::GuiBackend;

// ─────────────────────────────────────────────────────────────────────────────
// Chat API Context (minimal state for chat-only endpoints)
// ─────────────────────────────────────────────────────────────────────────────

/// Minimal application context for chat-only API.
///
/// This is a subset of the full `AxumContext` containing only what's needed
/// for chat history CRUD operations and chat completion proxying.
pub struct ChatApiContext {
    /// Core application services (provides chat_history()).
    pub core: Arc<AppCore>,
    /// GUI backend facade (provides list_servers() for port validation).
    pub gui: Arc<GuiBackend>,
}

/// Shared state type for chat API handlers.
pub type ChatState = Arc<ChatApiContext>;

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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
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
/// - `/api/conversations/:id` - Get/update/delete conversation
/// - `/api/conversations/:id/messages` - Get messages for conversation
/// - `/api/messages` - Save new message
/// - `/api/messages/:id` - Update/delete message
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
pub fn chat_routes(state: ChatState) -> Router {
    Router::new()
        // Conversation endpoints
        .route(
            "/api/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route(
            "/api/conversations/:id",
            get(get_conversation)
                .put(update_conversation)
                .delete(delete_conversation),
        )
        // Message endpoints
        .route("/api/conversations/:id/messages", get(get_messages))
        .route("/api/messages", post(save_message))
        .route(
            "/api/messages/:id",
            put(update_message).delete(delete_message),
        )
        // Chat completion proxy (forwards to llama-server)
        .route("/api/chat", post(proxy_chat))
        .with_state(state)
}

// ─────────────────────────────────────────────────────────────────────────────
// Conversation Handlers
// ─────────────────────────────────────────────────────────────────────────────

/// List all conversations.
/// GET /api/conversations
pub async fn list_conversations(
    State(state): State<ChatState>,
) -> Result<Json<Vec<Conversation>>, HttpError> {
    let conversations = state.core.chat_history().list_conversations().await?;
    Ok(Json(conversations))
}

/// Create a new conversation.
/// POST /api/conversations
pub async fn create_conversation(
    State(state): State<ChatState>,
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
    State(state): State<ChatState>,
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
    State(state): State<ChatState>,
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
    State(state): State<ChatState>,
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
    State(state): State<ChatState>,
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
    State(state): State<ChatState>,
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
    State(state): State<ChatState>,
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
    State(state): State<ChatState>,
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
async fn validate_port(state: &ChatState, port: u16) -> Result<(), HttpError> {
    // Basic range check
    if port < MIN_ALLOWED_PORT || port > MAX_ALLOWED_PORT {
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
    State(state): State<ChatState>,
    Json(request): Json<ChatProxyRequest>,
) -> Result<Response, HttpError> {
    // Validate the port
    validate_port(&state, request.port).await?;

    // Filter out messages with empty or whitespace-only content
    // EXCEPT: tool role messages (they return results) and assistant messages with tool_calls
    // This prevents Jinja template errors in llama-server
    let valid_messages: Vec<_> = request
        .messages
        .into_iter()
        .filter(|m| !m.content.trim().is_empty() || m.role == "tool" || m.tool_calls.is_some())
        .collect();

    if valid_messages.is_empty() {
        return Err(HttpError::BadRequest(
            "No valid messages to send. All messages have empty content.".into(),
        ));
    }

    // Build the llama-server URL
    let server_url = format!("http://127.0.0.1:{}/v1/chat/completions", request.port);

    // Build the forwarded request body
    let mut forward_body = serde_json::json!({
        "model": request.model,
        "messages": valid_messages,
        "stream": request.stream,
        "max_tokens": request.max_tokens,
        "temperature": request.temperature,
    });

    // Add tools if provided
    if let Some(tools) = &request.tools {
        if !tools.is_empty() {
            forward_body["tools"] = serde_json::json!(tools);
        }
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
            .map(|result| result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));

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
