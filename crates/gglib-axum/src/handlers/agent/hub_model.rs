//! Which model a device's turn on a hub chat runs on, and the port it is
//! served on: found running, or loaded as `/v1/models/{name}/load` loads it.

use axum::http::StatusCode;

use gglib_core::domain::agent::MADE_KEYS;
use gglib_core::domain::chat::{Conversation, Message};
use gglib_core::ports::{Admission, LaunchOverrides};

use super::AgentChatRequest;
use super::run::coded;
use crate::error::HttpError;
use crate::state::AppState;

/// The chat's model: the one it was made with, the one its settings name,
/// the one that made its last reply, or the hub's default, in that order.
///
/// # Errors
///
/// `no_model` (422) when none of them names one: nothing to wait for.
pub(super) async fn model_for(
    state: &AppState,
    conversation: &Conversation,
    rows: &[Message],
) -> Result<String, HttpError> {
    let catalogued = |id: Option<i64>| async move {
        let model = state.core.models().get_by_id(id?).await.ok().flatten()?;
        Some(model.name)
    };
    if let Some(name) = catalogued(conversation.model_id).await {
        return Ok(name);
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
    let default = state.core.settings().get().await.ok();
    if let Some(name) = catalogued(default.and_then(|s| s.default_model_id)).await {
        return Ok(name);
    }
    Err(coded(
        StatusCode::UNPROCESSABLE_ENTITY,
        "no_model",
        "the chat names no model and the hub has no default; choose one on the hub",
    ))
}

/// `chat` on the port `model` is served on, loading it first when it is not
/// running, as `/v1/models/{name}/load` does.
pub(super) async fn on_model(
    state: &AppState,
    model: &str,
    mut chat: AgentChatRequest,
) -> Result<AgentChatRequest, HttpError> {
    let servers = state.servers.list_servers().await;
    if let Some(server) = servers.iter().find(|s| s.model_name == model) {
        chat.port = server.port;
        return Ok(chat);
    }
    let default_ctx = state
        .core
        .settings()
        .get()
        .await
        .ok()
        .and_then(|s| s.default_context_size);
    let admission = state
        .runtime
        .admit(model, None, default_ctx, LaunchOverrides::default())
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
