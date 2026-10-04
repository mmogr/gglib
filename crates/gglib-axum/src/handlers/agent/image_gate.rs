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
//! A model of the paired machine is not asked about here. This machine's
//! catalogue does not hold it; its own proxy refuses the request by the
//! same code.

use axum::http::StatusCode;
use gglib_core::domain::Model;
use gglib_core::domain::agent::AgentMessage;
use gglib_core::request_pipeline::refuse_unless_can_see;

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

#[cfg(test)]
#[path = "image_gate_tests.rs"]
pub(super) mod image_gate_tests;
