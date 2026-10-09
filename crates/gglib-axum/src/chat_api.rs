//! Chat API routes and handlers.
//!
//! This module provides the endpoints for conversations and their messages,
//! and mounts the one that asks a model for a chat's title
//! (`handlers/chat_title.rs`).
//!
//! Chat handlers use the unified `AppState` from `routes.rs` and access
//! `core` and `gui` services through it.

use axum::extract::{Path, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::chat_changes;
use crate::error::HttpError;
use crate::handlers::chat_title;
use crate::state::AppState;
use gglib_core::domain::chat::{Conversation, ConversationSettings, NewConversation};
use gglib_core::ports::chat_history::ChatHistoryError;

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
    /// The model it is for, by its machine, kept as its settings' model.
    #[serde(default)]
    pub model: Option<gglib_core::domain::ModelRef>,
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

// ─────────────────────────────────────────────────────────────────────────────
// Router Factory
// ─────────────────────────────────────────────────────────────────────────────

/// Create a router with chat-only API endpoints.
///
/// This router provides:
/// - `/api/conversations` - List/create conversations
/// - `/api/conversations/{id}` - Get/update/delete conversation
/// - `/api/conversations/{id}/thread` - A chat with its branch points
/// - `/api/conversations/{id}/changes` - Edit, regenerate or branch a chat
/// - `/api/messages/{id}` - Delete a message and those after it
/// - `/api/chat` - Ask the model a chat runs on for the chat's title
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
        // A chat read with its branch points, and changed (ADR 0017)
        .route("/conversations/{id}/thread", get(chat_changes::thread))
        .route("/conversations/{id}/changes", post(chat_changes::change))
        .route("/messages/{id}", delete(delete_message))
        // A chat's title, asked of the model it runs on
        .route("/chat", post(chat_title::generate))
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

/// Create a new conversation. One made for a `model` keeps it as its
/// settings' model, which then decides its `model_id`: a conversation's
/// machine is fixed when it is made.
/// POST /api/conversations
pub(crate) async fn create_conversation(
    State(state): State<AppState>,
    Json(req): Json<CreateConversationRequest>,
) -> Result<Json<i64>, HttpError> {
    let settings = req.model.map(|model| ConversationSettings {
        model: Some(model),
        ..ConversationSettings::default()
    });
    let conv = NewConversation {
        title: req.title.unwrap_or_else(|| "New Conversation".to_string()),
        model_id: req.model_id,
        system_prompt: req.system_prompt,
        settings,
    };
    let id = state.core.chat_history().create_conversation(conv).await?;
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
        .ok_or_else(|| HttpError::NotFound(format!("Conversation not found: {id}")))?;
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

/// Delete a conversation and all its messages; refused while a reply to it
/// is still being written.
/// DELETE /api/conversations/:id
pub(crate) async fn delete_conversation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(), HttpError> {
    state.runs.refuse_if_live(id)?;
    state.core.chat_history().delete_conversation(id).await?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Message Handlers
// ─────────────────────────────────────────────────────────────────────────────

/// Delete a message and all subsequent messages in the conversation, all or
/// none; refused while a reply to the conversation is still being written.
/// DELETE /api/messages/:id
pub(crate) async fn delete_message(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<i64>, HttpError> {
    let history = state.core.chat_history();
    let conversation_id = history
        .conversation_of_message(id)
        .await?
        .ok_or(ChatHistoryError::MessageNotFound(id))?;
    state.runs.refuse_if_live(conversation_id)?;
    let deleted_count = history.delete_message_and_subsequent(id).await?;
    Ok(Json(deleted_count))
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "chat_api_tests.rs"]
mod chat_api_tests;
