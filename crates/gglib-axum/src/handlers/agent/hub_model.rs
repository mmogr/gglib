//! Which model a device's turn on a hub chat runs on, and the port it is
//! served on: found running, or loaded as `/v1/models/{name}/load` loads it.
//!
//! A chat that stored its model ([`ConversationSettings::model`]) names it
//! by its machine and its id there. One that ran here is run by that id, so
//! a second model of the same name cannot answer it. One that ran on the
//! machine this one is paired with is refused: the hub never resolves the
//! other machine's model as a name in its own catalogue.
//!
//! [`ConversationSettings::model`]: gglib_core::domain::chat::ConversationSettings::model

use std::cmp::Reverse;

use axum::http::StatusCode;

use gglib_app_services::types::ServerInfo;
use gglib_core::domain::Machine;
use gglib_core::domain::agent::MADE_KEYS;
use gglib_core::domain::chat::{Conversation, Message};
use gglib_core::ports::{Admission, LaunchOverrides};

use super::AgentChatRequest;
use super::run::coded;
use crate::error::HttpError;
use crate::state::AppState;

/// The chat's model, as an identifier the hub's catalogue resolves: the one
/// its settings store by id, the one it was made with, the one its settings
/// name, the one that made its last reply, the one running on the hub, or
/// the hub's default, in that order. A model known by its id is given by
/// its id.
///
/// # Errors
///
/// `conflict` (409) for a chat that ran on the paired machine; `no_model`
/// (422) when nothing names one: nothing to wait for.
pub(super) async fn model_for(
    state: &AppState,
    conversation: &Conversation,
    rows: &[Message],
) -> Result<String, HttpError> {
    let running = state.servers.list_servers().await;
    choose(state, conversation, rows, &running).await
}

/// [`model_for`], given the servers `running` on the hub.
pub(super) async fn choose(
    state: &AppState,
    conversation: &Conversation,
    rows: &[Message],
    running: &[ServerInfo],
) -> Result<String, HttpError> {
    if let Some(model) = conversation
        .settings
        .as_ref()
        .and_then(|s| s.model.as_ref())
    {
        return match &model.machine {
            Machine::Local => Ok(model.id.to_string()),
            Machine::Paired { .. } => Err(ran_elsewhere()),
        };
    }
    let catalogued = |id: Option<i64>| async move {
        let model = state.core.models().get_by_id(id?).await.ok().flatten()?;
        Some(model.id.to_string())
    };
    if let Some(id) = catalogued(conversation.model_id).await {
        return Ok(id);
    }
    let named = conversation
        .settings
        .as_ref()
        .and_then(|s| s.model_name.clone())
        .or_else(|| {
            rows.iter().rev().find_map(|row| {
                let name = row.metadata.as_ref()?.get(MADE_KEYS.model)?.as_str()?;
                Some(name.to_owned())
            })
        })
        .filter(|name| !name.trim().is_empty());
    if let Some(name) = named {
        return Ok(name);
    }
    if let Some(id) = latest(running) {
        return Ok(id);
    }
    let default = state.core.settings().get().await.ok();
    if let Some(id) = catalogued(default.and_then(|s| s.default_model_id)).await {
        return Ok(id);
    }
    Err(coded(
        StatusCode::UNPROCESSABLE_ENTITY,
        "no_model",
        "this chat has no model and nothing is running on the hub: start a model there",
    ))
}

/// A chat that ran on the paired machine, refused in fixed text: the
/// device asking has no business with that machine, so neither its name nor
/// its fingerprint is told to it.
fn ran_elsewhere() -> HttpError {
    HttpError::Conflict(
        "this chat ran on another machine, and its model is that machine's, not this one's"
            .to_owned(),
    )
}

/// The model id of the server started last, the lowest port among those
/// started together; none when nothing runs.
fn latest(running: &[ServerInfo]) -> Option<String> {
    let last = running
        .iter()
        .max_by_key(|s| (s.started_at, Reverse(s.port)))?;
    Some(last.model_id.to_string())
}

/// The port of the server running catalogue model `id`, if one is.
fn serving(running: &[ServerInfo], id: i64) -> Option<u16> {
    running.iter().find(|s| s.model_id == id).map(|s| s.port)
}

/// `chat` on the port `model` is served on, loading it first when it is not
/// running, as `/v1/models/{name}/load` does. A running server is matched by
/// the id `model` resolves to, and a load is asked for by that id, so a
/// second model of the same name is neither found nor loaded in its place.
pub(super) async fn on_model(
    state: &AppState,
    model: &str,
    mut chat: AgentChatRequest,
) -> Result<AgentChatRequest, HttpError> {
    let id = state
        .core
        .models()
        .get(model)
        .await
        .ok()
        .flatten()
        .map(|m| m.id);
    let servers = state.servers.list_servers().await;
    if let Some(port) = id.and_then(|id| serving(&servers, id)) {
        chat.port = port;
        return Ok(chat);
    }
    let model = id.map_or_else(|| model.to_owned(), |id| id.to_string());
    let default_ctx = state
        .core
        .settings()
        .get()
        .await
        .ok()
        .and_then(|s| s.default_context_size);
    let admission = state
        .runtime
        .admit(&model, None, default_ctx, LaunchOverrides::default())
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "a device's turn could not load its chat's model");
            coded(
                StatusCode::SERVICE_UNAVAILABLE,
                "model_unavailable",
                "the chat's model could not be loaded on the hub",
            )
        })?;
    chat.port = Admission::into_target(admission).port;
    Ok(chat)
}

#[cfg(test)]
#[path = "hub_model_tests.rs"]
mod tests;
