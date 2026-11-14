//! Chat API routes and handlers.
//!
//! This module provides the endpoints for conversations and their messages,
//! and mounts the one that asks a model for a chat's title
//! (`handlers/chat_title.rs`).
//!
//! Chat handlers use the unified `AppState` from `routes.rs` and access
//! `core` and `gui` services through it.

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::chat_changes;
use crate::error::HttpError;
use crate::handlers::agent::{keep_a_model, keep_machine};
use crate::handlers::chat_title;
use crate::state::AppState;
use gglib_core::domain::chat::{Conversation, ConversationSettings, NewConversation};
use gglib_core::domain::{ConversationSettingsPatch, Machine, ModelRef};
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
///
/// `settings` changes the settings its runs read, each one sent and no
/// other: its model, its tools and its tool limits. Left out, or `null`, it
/// changes none.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct UpdateConversationRequest {
    pub title: Option<String>,
    #[cfg_attr(feature = "ts-bindings", ts(type = "string | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    #[expect(
        clippy::option_option,
        reason = "the three states are the API: omitted leaves the prompt, null clears it, \
                  a string sets it, and double_option deserializes exactly that"
    )]
    pub system_prompt: Option<Option<String>>,
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ConversationSettingsPatch>,
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
/// - `/api/conversations/{id}/pictures` - Keep a picture beside the one it was made from
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
        // A picture drawn again, kept beside the one it was made from
        .route(
            "/conversations/{id}/pictures",
            post(chat_changes::keep_picture),
        )
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

/// Update a conversation: its title, its system prompt, and the settings
/// its runs read, in one write of what was sent. A limit outside a run's
/// bounds, and a model of this machine its catalogue does not hold, are a
/// 400; a model of another machine than the one the conversation ran on is
/// refused as a run on it is. A body that cannot be read, a setting the
/// patch does not name among them, is a 400 coded `invalid_request`. A
/// change to settings stored as text that is not a JSON object is a 409. A
/// refused update changes nothing.
/// PUT /api/conversations/:id
pub(crate) async fn update_conversation(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    req: Result<Json<UpdateConversationRequest>, JsonRejection>,
) -> Result<(), HttpError> {
    let Json(req) = req.map_err(|e| HttpError::Coded {
        status: StatusCode::BAD_REQUEST,
        code: "invalid_request",
        message: e.body_text(),
    })?;
    let patch = req.settings.unwrap_or_default();
    if let Some(reason) = patch.refusal() {
        return Err(HttpError::BadRequest(reason));
    }
    let model = patch.model.clone().flatten();
    let model_name = match &model {
        Some(model) => model_name(&state, model).await?,
        None => None,
    };
    match &patch.model {
        Some(Some(model)) => keep_machine(&state.core, id, &model.machine).await?,
        Some(None) => keep_a_model(&state.core, id).await?,
        None => {}
    }
    state
        .core
        .chat_history()
        .update_conversation(id, req.title, req.system_prompt, patch, model_name)
        .await?;
    Ok(())
}

/// The name `model` goes by: a model of this machine's catalogue, by its
/// name there; none for the paired machine's, whose name is not read here
/// (its next run names it).
///
/// # Errors
///
/// A 400 for a model of this machine the catalogue does not hold.
async fn model_name(state: &AppState, model: &ModelRef) -> Result<Option<String>, HttpError> {
    if model.machine != Machine::Local {
        return Ok(None);
    }
    let held = state.core.models().get_by_id(model.id).await?;
    let held = held.ok_or_else(|| {
        HttpError::BadRequest(format!("no model has id {} on this machine", model.id))
    })?;
    Ok(Some(held.name))
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
