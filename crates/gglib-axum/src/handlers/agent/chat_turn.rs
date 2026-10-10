//! A chat run with builtins: the proxy's
//! `PUT /v1/runs/{id}?kind=chat&tools=builtin[&draw=true]`, for a chat a
//! paired phone keeps itself, run here because the agent loop, its slot and
//! the image tool are the daemon's.
//!
//! The body is the phone's unchanged `OpenAI` chat request. Its `messages`
//! are the whole history (`gglib_core`'s `parse_openai_messages`), and each
//! image sent inline as a data URL is stored here as an attachment, since
//! the loop names images by id; nothing links those to a chat, so the
//! unlinked sweep removes them a day later, at the next daemon start. The
//! run is on the request's `model`, found or loaded inside the run as a
//! hub turn's is, with this machine's sampling for it: what the request
//! says of sampling, `max_tokens` included, is not read, as on a hub turn.
//! One thing of it is: the phone's Thinking choice. A body that turns
//! thinking off as a phone-kept chat does on the chat route,
//! `"reasoning_budget_tokens": 0`, runs with a thinking budget of `0`; any
//! other budget is sampling, and is not read. Its only tool is the
//! image tool, and only with `draw`. Nothing is written to any chat: the
//! phone keeps the conversation, and reads the reply from the run, whose
//! events are the agent loop's (`frames: agent`).
//!
//! Nothing here logs or returns a message: a history that cannot be read
//! is refused by the index of the message, never its text.

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::Value;

use gglib_core::domain::AttachmentId;
use gglib_core::domain::agent::{inline_images, into_agent_messages, parse_openai_messages};
use gglib_core::domain::runs::RunKind;
use gglib_core::ports::{Created, RunScope};

use super::AgentChatRequest;
use super::compose::{DRAW_TOOL, refuse_unavailable_drawing, take_permit};
use super::hub_turn::{late_on, shown_model};
use super::launch::{Transcript, launch_turn};
use super::run::coded;
use crate::error::HttpError;
use crate::state::AppState;

fn invalid(message: impl Into<String>) -> HttpError {
    coded(StatusCode::BAD_REQUEST, "invalid_request", message)
}

/// Start `device`'s chat run `id` with builtins, or answer with its run
/// that has the id.
///
/// # Errors
///
/// What [`read`] refuses; `agent_busy` (429); and the runs' own. What is
/// refused once the run exists ends it `failed` (`model_unavailable`), as a
/// hub turn's.
pub(super) async fn start(
    state: &AppState,
    device: &str,
    id: &str,
    body: Value,
    draw: bool,
) -> Result<Created, HttpError> {
    let scope = RunScope::Device(device.to_owned());
    if let Some(info) = state.runs.existing(&scope, id)? {
        return Ok(Created {
            info,
            created: false,
        });
    }
    let (model, chat) = read(state, &body, draw).await?;
    let permit = take_permit(state).ok_or_else(|| {
        coded(
            StatusCode::TOO_MANY_REQUESTS,
            "agent_busy",
            "all agent loop slots are in use; try again later",
        )
    })?;
    let shown = shown_model(state, &model).await;
    let late = late_on(Arc::clone(state), model, chat);
    // Saved to no chat: the phone keeps this conversation.
    let transcript = Transcript {
        conversation_id: None,
        answer_saved: false,
        remember: None,
    };
    let run = (RunKind::Chat, shown);
    launch_turn(state, id, scope, run, transcript, late, permit)
}

/// An `OpenAI` chat request read as the loop's: the catalogue model it runs
/// on, and the request the loop is composed from, its history's inline
/// images stored and named by id.
///
/// # Errors
///
/// `invalid_request` (400) for a body that names no model or whose history
/// cannot be read; `image_model_cannot_chat` (400); `drawing_unavailable`
/// (400) for `draw` where nothing can draw; the attachment store's refusal
/// of an inline image; `attachment_not_found`, `request_images_too_large`
/// and `model_cannot_read_images` (400) as a hub turn has them. Only an
/// inline image is ever written, and only once every check that needs no
/// image has passed.
pub(super) async fn read(
    state: &AppState,
    body: &Value,
    draw: bool,
) -> Result<(String, AgentChatRequest), HttpError> {
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid("a chat run with builtins names its `model`"))?;
    let turns = parse_openai_messages(body.get("messages").unwrap_or(&Value::Null))
        .map_err(|e| invalid(e.to_string()))?;
    let model = catalogued(state, model).await;
    super::image_gate::chats_named(state, &model).await?;

    let mut chat = AgentChatRequest {
        port: 0,
        far: None,
        messages: Vec::new(),
        config: None,
        // The image tool and nothing else, and that only with Draw.
        tool_filter: Some(draw.then(|| DRAW_TOOL.to_owned()).into_iter().collect()),
        model: None,
        // The request's own sampling is not read: the model's, as on a hub
        // turn. Its Thinking choice is: off, or the model's default.
        reasoning_effort: None,
        reasoning_budget_tokens: thinking_off(body).then_some(0),
        draw,
    };
    refuse_unavailable_drawing(state, &chat).await?;

    // Each inline image stored once, under the hash of its bytes; the same
    // picture sent again with the next turn is the same attachment.
    let attachments = state.core.attachments();
    let mut ids: Vec<AttachmentId> = Vec::new();
    for bytes in inline_images(&turns) {
        ids.push(attachments.ingest(bytes).await?.info.id);
    }
    chat.messages = into_agent_messages(turns, ids).map_err(|e| invalid(e.to_string()))?;
    attachments.check_request(&chat.messages).await?;
    super::image_gate::named(state, &model, &chat.messages).await?;
    Ok((model, chat))
}

/// Whether `body` turns thinking off, as a client that keeps its own chats
/// says it on the chat route: `"reasoning_budget_tokens": 0`
/// (docs/clients.md, "Thinking"). A budget above zero is sampling.
fn thinking_off(body: &Value) -> bool {
    body.get("reasoning_budget_tokens").and_then(Value::as_i64) == Some(0)
}

/// `named` as an identifier this catalogue resolves: itself, or, for a
/// `{model}:{profile}` name the proxy lists, the model before the suffix
/// when only that is in the catalogue. A name the catalogue does not hold
/// is kept, and its load says so.
async fn catalogued(state: &AppState, named: &str) -> String {
    let models = state.core.models();
    if models.get(named).await.ok().flatten().is_some() {
        return named.to_owned();
    }
    if let Some((base, _profile)) = named.rsplit_once(':')
        && models.get(base).await.ok().flatten().is_some()
    {
        return base.to_owned();
    }
    named.to_owned()
}

#[cfg(test)]
#[path = "chat_turn_tests.rs"]
mod tests;
