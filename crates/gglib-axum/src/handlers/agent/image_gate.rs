//! The refusal of a run whose messages carry an image, for a model of this
//! machine that cannot read one.
//!
//! The rule, the code and the words are core's
//! ([`gglib_core::request_pipeline::refuse_unless_can_see`]); this is where
//! the daemon asks it, before a model is loaded or a row is written: a run
//! on a server already started, by the model that server serves, and a
//! device's turn on a hub chat, by the model the chat runs on. The whole
//! history counts, since it is all sent again each turn.
//!
//! The refusal of any run for a model that draws images is here too, by the
//! same doors and before the same things: `sd-server` serves such a model
//! and cannot chat, so a device's turn on a hub chat whose model draws, and
//! a page's run on a port that serves one, are refused with
//! `image_model_cannot_chat` before anything is loaded, held or written.
//! The rule, the code and the words are core's
//! ([`gglib_core::request_pipeline::refuse_unless_chats`]).
//!
//! A model of the paired machine is not asked about here. This machine's
//! catalogue does not hold it; its own proxy refuses the request by the
//! same code.

use axum::http::StatusCode;
use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::{Model, RuntimeKind};
use gglib_core::request_pipeline::{refuse_unless_can_see, refuse_unless_chats};

use crate::error::HttpError;
use crate::state::AppState;

/// Refuse `messages` for `model` when one carries an image and the model
/// has no projector. A model the catalogue does not hold is not judged: it
/// is refused, or not, by whatever loads it.
fn readable_by(model: Option<&Model>, messages: &[AgentMessage]) -> Result<(), HttpError> {
    let Some(model) = model else {
        return Ok(());
    };
    let has_images = messages.iter().any(AgentMessage::has_images);
    refuse_unless_can_see(model.image_input(), has_images).map_err(|refusal| HttpError::Coded {
        status: StatusCode::BAD_REQUEST,
        code: refusal.code(),
        message: refusal.message(&model.name),
    })
}

/// [`readable_by`] the catalogue model `id`: the one a running server
/// serves.
///
/// # Errors
///
/// `model_cannot_read_images` (400).
pub(super) async fn served(
    state: &AppState,
    id: i64,
    messages: &[AgentMessage],
) -> Result<(), HttpError> {
    let model = state.core.models().get_by_id(id).await.ok().flatten();
    readable_by(model.as_ref(), messages)
}

/// [`readable_by`] the model `identifier` names in the catalogue: the one a
/// hub chat runs on, as `hub_model::model_for` gives it.
///
/// # Errors
///
/// `model_cannot_read_images` (400).
pub(super) async fn named(
    state: &AppState,
    identifier: &str,
    messages: &[AgentMessage],
) -> Result<(), HttpError> {
    let model = state.core.models().get(identifier).await.ok().flatten();
    readable_by(model.as_ref(), messages)
}

/// Refuse a run for model `name`, served by `runtime`, when it draws.
///
/// # Errors
///
/// `image_model_cannot_chat` (400).
pub(super) fn chats(runtime: RuntimeKind, name: &str) -> Result<(), HttpError> {
    refuse_unless_chats(runtime).map_err(|refusal| HttpError::Coded {
        status: StatusCode::BAD_REQUEST,
        code: refusal.code(),
        message: refusal.message(name),
    })
}

/// [`chats`] for the model `identifier` names in the catalogue: the one a
/// hub chat runs on. A model the catalogue does not hold is not judged.
///
/// # Errors
///
/// `image_model_cannot_chat` (400).
pub(super) async fn chats_named(state: &AppState, identifier: &str) -> Result<(), HttpError> {
    match state.core.models().get(identifier).await.ok().flatten() {
        Some(model) => chats(model.runtime(), &model.name),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "image_gate_chat_tests.rs"]
mod image_gate_chat_tests;
#[cfg(test)]
#[path = "image_gate_tests.rs"]
pub(super) mod image_gate_tests;
