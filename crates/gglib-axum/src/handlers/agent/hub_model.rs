//! Which model a turn on one of this machine's chats runs on, and the port
//! it is served on: found running, or loaded as `/v1/gglib/models/{name}/load`
//! loads it.
//!
//! A chat that stored its model ([`ConversationSettings::model`]) names it
//! by its machine and its id there. One that ran here is run by that id, so
//! a renamed model still answers it and a second model of the same name
//! cannot. A name is a label, and is never looked up ([`model_for`]). One
//! stored as the paired machine's runs there on this machine's own turn
//! ([`paired`]); a paired device's turn on it is refused once the chat has a
//! saved message, and before that the model was only chosen, so the turn
//! runs on this machine's as a chat that names none does. Either way the hub
//! never resolves the other machine's model as a name in its own catalogue.
//!
//! [`ConversationSettings::model`]: gglib_core::domain::chat::ConversationSettings::model

use std::cmp::Reverse;

use axum::http::StatusCode;

use gglib_app_services::types::ServerInfo;
use gglib_core::domain::chat::{Conversation, Message};
use gglib_core::domain::{Machine, ModelCapabilities, ModelRef, RuntimeKind};
use gglib_core::ports::{Admission, LaunchOverrides, ModelRuntimeError, RunScope};

use super::launch::Loading;
use super::run::coded;
use crate::error::HttpError;
use crate::state::AppState;

/// The paired machine's model a turn in `scope` runs on: the one the chat
/// stored, on this machine's own turn. None on a paired device's turn, which
/// never reaches the paired machine, and for a chat that stored none of its.
pub(super) fn paired(scope: &RunScope, conversation: &Conversation) -> Option<ModelRef> {
    if scope.device().is_some() {
        return None;
    }
    let model = conversation.settings.as_ref()?.model.as_ref()?;
    matches!(model.machine, Machine::Paired { .. }).then(|| model.clone())
}

/// Whether the catalogue's `model` calls tools, as its capability says; a
/// model the catalogue does not hold is given the benefit of the doubt, as
/// the page gives one whose capability it does not know.
pub(super) async fn calls_tools(state: &AppState, model: &str) -> bool {
    let found = state.core.models().get(model).await.ok().flatten();
    found.is_none_or(|m| {
        m.capabilities
            .contains(ModelCapabilities::SUPPORTS_TOOL_CALLS)
    })
}

/// The model a turn on `conversation` runs on: the paired machine's `far`
/// ([`paired`]), by the name the chat has for it, else its id there; else
/// this machine's ([`model_for`]), which must be one that chats.
///
/// # Errors
///
/// What [`model_for`] refuses, and `image_model_cannot_chat` (400).
pub(super) async fn turn_model(
    state: &AppState,
    far: Option<&ModelRef>,
    conversation: &Conversation,
    rows: &[Message],
) -> Result<String, HttpError> {
    if let Some(far) = far {
        // Nothing of the paired machine's model is looked up here: its id
        // and name are that machine's. Its proxy refuses what it cannot do.
        let settings = conversation.settings.as_ref();
        let named = settings.and_then(|s| s.model_name.clone());
        let named = named.filter(|n| !n.trim().is_empty());
        return Ok(named.unwrap_or_else(|| far.id.to_string()));
    }
    let model = model_for(state, conversation, rows).await?;
    // Once the chat's model is known, before `on_model` would load it: a
    // model that draws is served by sd-server, which cannot chat.
    super::image_gate::chats_named(state, &model).await?;
    Ok(model)
}

/// The chat's model, as an id in the hub's catalogue: the one its settings
/// store, the one it was made with, the one running on the hub, or the hub's
/// default, in that order. The name its settings or its last reply give is
/// never looked up, so a model renamed since still answers, and a chat that
/// names only a name runs as one that names no model.
///
/// A chat with no saved message that stored the paired machine's model has
/// run nowhere yet: it takes the one running here, or the hub's default,
/// and never the name it stored, which is that machine's.
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
        match &model.machine {
            Machine::Local => return Ok(model.id.to_string()),
            // Chosen on the paired machine before the chat had run: nothing
            // of it names a model here.
            Machine::Paired { .. } if rows.is_empty() => {}
            Machine::Paired { .. } => return Err(ran_elsewhere()),
        }
    }
    let catalogued = |id: Option<i64>| async move {
        let model = state.core.models().get_by_id(id?).await.ok().flatten()?;
        Some(model.id.to_string())
    };
    if let Some(id) = catalogued(conversation.model_id).await {
        return Ok(id);
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

/// The model id of the chat server started last, the lowest port among
/// those started together; none when no chat server runs. An `sd-server`
/// is passed over: a chat cannot run on a model that draws.
fn latest(running: &[ServerInfo]) -> Option<String> {
    let last = running
        .iter()
        .filter(|s| s.runtime == RuntimeKind::Llama)
        .max_by_key(|s| (s.started_at, Reverse(s.port)))?;
    Some(last.model_id.to_string())
}

/// The port of the server running catalogue model `id`, if one is.
fn serving(running: &[ServerInfo], id: i64) -> Option<u16> {
    running.iter().find(|s| s.model_id == id).map(|s| s.port)
}

/// A turn's model once it is found running or loaded: its catalogue id, the
/// one the turn dials the endpoint with, and the port it was on then, which
/// only says where to read the context it was launched with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Loaded {
    pub(super) id: i64,
    pub(super) port: u16,
}

/// The port `model` is served on, loading it first when it is not
/// running, as `/v1/gglib/models/{name}/load` does. A running server is matched by
/// the id `model` resolves to, and a load is asked for by that id, so a
/// second model of the same name is neither found nor loaded in its place.
/// `loading` is told, with the catalogue's name for the model, before a
/// load is asked for, which may take minutes or wait behind an image render.
///
/// The load's lease is dropped once it is in: the turn's requests go
/// through this machine's endpoint, which admits each of them, so nothing
/// here keeps the model resident between them, and the port may serve
/// another model, or none, by the time the turn dials. So the turn is told
/// the model by its id, which is what it dials with, never by its port.
///
/// # Errors
///
/// `model_not_found` (404), naming `model`, when this machine has no such
/// model, and `model_unavailable` (503) when it has one it cannot load.
pub(super) async fn on_model(
    state: &AppState,
    model: &str,
    loading: &Loading,
) -> Result<Loaded, HttpError> {
    let found = state.core.models().get(model).await.ok().flatten();
    let id = found.as_ref().map(|m| m.id);
    let servers = state.servers.list_servers().await;
    if let Some((id, port)) = id.and_then(|id| Some((id, serving(&servers, id)?))) {
        return Ok(Loaded { id, port });
    }
    loading.waiting(found.as_ref().map_or(model, |m| m.name.as_str()));
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
        .map_err(|e| match e {
            ModelRuntimeError::ModelNotFound(model) => coded(
                StatusCode::NOT_FOUND,
                "model_not_found",
                format!("no model named {model} on this machine"),
            ),
            e => {
                tracing::warn!(error = %e, "a turn could not load its chat's model");
                coded(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "model_unavailable",
                    "the chat's model could not be loaded on the hub",
                )
            }
        })?;
    let target = Admission::into_target(admission);
    Ok(Loaded {
        // The catalogue's id; a name it does not hold, by what was loaded.
        id: id.unwrap_or_else(|| i64::from(target.model_id)),
        port: target.port,
    })
}

#[cfg(test)]
#[path = "hub_model_tests.rs"]
mod tests;
