//! Chat history commands.
//!
//! Commands for managing chat conversations and messages.
//! These mirror the Axum handlers for parity across adapters.

use crate::app::AppState;
use gglib_core::domain::chat::{Conversation, Message, MessageRole, NewMessage};
use serde::Deserialize;

// ─────────────────────────────────────────────────────────────────────────────
// Request DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Request for creating a new conversation.
#[derive(Debug, Deserialize)]
pub struct CreateConversationRequest {
    pub title: Option<String>,
    pub model_id: Option<i64>,
    pub system_prompt: Option<String>,
}

/// Request for updating a conversation.
#[derive(Debug, Deserialize)]
pub struct UpdateConversationRequest {
    pub title: Option<String>,
    pub system_prompt: Option<Option<String>>,
}

/// Request for saving a new message.
#[derive(Debug, Deserialize)]
pub struct SaveMessageRequest {
    pub conversation_id: i64,
    pub role: String,
    pub content: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Conversation Commands
// ─────────────────────────────────────────────────────────────────────────────

/// List all conversations.
#[tauri::command]
pub async fn list_conversations(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Conversation>, String> {
    state
        .core
        .chat_history()
        .list_conversations()
        .await
        .map_err(|e| format!("Failed to list conversations: {}", e))
}

/// Create a new conversation.
#[tauri::command]
pub async fn create_conversation(
    request: CreateConversationRequest,
    state: tauri::State<'_, AppState>,
) -> Result<i64, String> {
    let title = request
        .title
        .unwrap_or_else(|| "New Conversation".to_string());
    state
        .core
        .chat_history()
        .create_conversation(title, request.model_id, request.system_prompt)
        .await
        .map_err(|e| format!("Failed to create conversation: {}", e))
}

/// Get a single conversation by ID.
#[tauri::command]
pub async fn get_conversation(
    id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<Conversation, String> {
    state
        .core
        .chat_history()
        .get_conversation(id)
        .await
        .map_err(|e| format!("Failed to get conversation: {}", e))?
        .ok_or_else(|| format!("Conversation not found: {}", id))
}

/// Update a conversation.
#[tauri::command]
pub async fn update_conversation(
    id: i64,
    request: UpdateConversationRequest,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .core
        .chat_history()
        .update_conversation(id, request.title, request.system_prompt)
        .await
        .map_err(|e| format!("Failed to update conversation: {}", e))
}

/// Delete a conversation and all its messages.
#[tauri::command]
pub async fn delete_conversation(
    id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .core
        .chat_history()
        .delete_conversation(id)
        .await
        .map_err(|e| format!("Failed to delete conversation: {}", e))
}

// ─────────────────────────────────────────────────────────────────────────────
// Message Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get all messages for a conversation.
#[tauri::command]
pub async fn get_messages(
    conversation_id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Message>, String> {
    state
        .core
        .chat_history()
        .get_messages(conversation_id)
        .await
        .map_err(|e| format!("Failed to get messages: {}", e))
}

/// Save a new message.
#[tauri::command]
pub async fn save_message(
    request: SaveMessageRequest,
    state: tauri::State<'_, AppState>,
) -> Result<i64, String> {
    let role = MessageRole::parse(&request.role)
        .ok_or_else(|| format!("Invalid message role: {}", request.role))?;

    state
        .core
        .chat_history()
        .save_message(NewMessage {
            conversation_id: request.conversation_id,
            role,
            content: request.content,
        })
        .await
        .map_err(|e| format!("Failed to save message: {}", e))
}

/// Update a message's content.
#[tauri::command]
pub async fn update_message(
    id: i64,
    content: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state
        .core
        .chat_history()
        .update_message(id, content)
        .await
        .map_err(|e| format!("Failed to update message: {}", e))
}

/// Delete a message and all subsequent messages in the conversation.
#[tauri::command]
pub async fn delete_message(
    id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<i64, String> {
    state
        .core
        .chat_history()
        .delete_message_and_subsequent(id)
        .await
        .map_err(|e| format!("Failed to delete message: {}", e))
}
