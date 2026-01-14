//! Chat history handlers - CRUD operations for conversations and messages.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;

use crate::error::HttpError;
use crate::state::AppState;
use gglib_core::domain::chat::{Conversation, Message, MessageRole, NewMessage};

// ─────────────────────────────────────────────────────────────────────────────
// Request DTOs (adapter-local)
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
/// GET /api/conversations/{id}
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
/// PUT /api/conversations/{id}
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
/// DELETE /api/conversations/{id}
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
/// GET /api/conversations/{id}/messages
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
/// PUT /api/messages/{id}
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
/// DELETE /api/messages/{id}
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
