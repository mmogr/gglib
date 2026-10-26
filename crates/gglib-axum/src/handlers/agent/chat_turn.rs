//! A chat run with builtins: the proxy's
//! `PUT /v1/runs/{id}?kind=chat&tools=<id>,<id>[&draw=true[&image_model=<id>]]`,
//! or `tools=none`, for a chat a paired phone keeps itself, run here because
//! the agent loop, its slot and the image tool are the daemon's. Its query
//! is a [`ChatQuery`]: whether it draws, and which of the servers open to
//! paired devices it may use, those whose ids it names, or none.
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
//! other budget is sampling, and is not read. It is offered the tools of
//! the MCP servers open to paired devices that its query lets it use,
//! settled when the run starts (`device_tools`, ADR 0025), and none of
//! them, with no server listed or started, when it may use none; and the
//! image tool with `draw` either way, drawing with the `image_model` the
//! query names when it names one. Without `draw` a run that may use no
//! server calls no tool. A `PUT` under the id of a run already started
//! answers with that run before its body is read as a chat request
//! (`state.runs.existing` in [`start`]), so a run keeps the servers it
//! started with; its query has been read by then, and one the proxy cannot
//! read is refused there. Nothing is written to any chat: the phone keeps the
//! conversation, and reads the reply from the run, whose events are the
//! agent loop's (`frames: agent`).
//!
//! Nothing here logs or returns a message: a history that cannot be read
//! is refused by the index of the message, never its text.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::Value;

use gglib_core::domain::AttachmentId;
use gglib_core::domain::agent::{inline_images, into_agent_messages, parse_openai_messages};
use gglib_core::domain::runs::RunKind;
use gglib_core::ports::{Created, RunScope};

use super::AgentChatRequest;
use super::compose::{refuse_unavailable_drawing, take_permit};
use super::device_tools::Asked;
use super::hub_turn::{late_on, shown_model};
use super::launch::{Transcript, launch_turn};
use super::run::coded;
use crate::error::HttpError;
use crate::state::AppState;

fn invalid(message: impl Into<String>) -> HttpError {
    coded(StatusCode::BAD_REQUEST, "invalid_request", message)
}

/// What the run's query says of drawing.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Drawing {
    /// `&draw=true`: the message was sent with Draw pressed.
    pub(super) draw: bool,
    /// `&image_model=<id>`: the image model the person chose for it.
    pub(super) image_model: Option<u32>,
}

#[cfg(test)]
impl From<bool> for Drawing {
    /// `draw` alone, with no image model chosen.
    fn from(draw: bool) -> Self {
        Self {
            draw,
            image_model: None,
        }
    }
}

/// What a phone-kept chat run's query says: of drawing, and which open
/// servers it may use.
#[derive(Debug, Clone, Default)]
pub(super) struct ChatQuery {
    /// Draw, and the image model chosen for it.
    pub(super) drawing: Drawing,
    /// The ids of the servers open to paired devices the run may use, of
    /// those the query names; empty, none. Draw offers the image tool
    /// whichever they are.
    pub(super) servers: BTreeSet<i64>,
}

#[cfg(test)]
impl From<bool> for ChatQuery {
    /// `draw` alone, with no image model chosen and no server.
    fn from(draw: bool) -> Self {
        Self {
            drawing: draw.into(),
            servers: BTreeSet::new(),
        }
    }
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
    query: ChatQuery,
) -> Result<Created, HttpError> {
    let ChatQuery { drawing, servers } = query;
    let scope = RunScope::Device(device.to_owned());
    if let Some(info) = state.runs.existing(&scope, id)? {
        return Ok(Created {
            info,
            created: false,
        });
    }
    let (model, chat) = read(state, &body, drawing).await?;
    let permit = take_permit(state).ok_or_else(|| {
        coded(
            StatusCode::TOO_MANY_REQUESTS,
            "agent_busy",
            "all agent loop slots are in use; try again later",
        )
    })?;
    let shown = shown_model(state, &model).await;
    let late = late_on(
        Arc::clone(state),
        model,
        chat,
        Asked::of_phone(drawing.draw, servers),
    );
    // Saved to no chat: the phone keeps this conversation.
    let transcript = Transcript {
        conversation_id: None,
        answer_saved: false,
        remember: None,
        remember_folder: None,
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
/// (400) for `draw` where nothing can draw, `invalid_request` (400) for an
/// `image_model` without `draw`, and `model_not_found`,
/// `not_an_image_model` or `image_model_incomplete` (400) for one that
/// cannot draw; the attachment store's refusal
/// of an inline image; `attachment_not_found`, `request_images_too_large`
/// and `model_cannot_read_images` (400) as a hub turn has them. Only an
/// inline image is ever written, and only once every check that needs no
/// image has passed.
pub(super) async fn read(
    state: &AppState,
    body: &Value,
    Drawing { draw, image_model }: Drawing,
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
        // The floor: the image tool with Draw. The servers open to devices
        // are read when the run starts, not for a request that is refused.
        tool_filter: Some(Asked::of_phone(draw, BTreeSet::new()).floor()),
        model: None,
        // The request's own sampling is not read: the model's, as on a hub
        // turn. Its Thinking choice is: off, or the model's default.
        reasoning_effort: None,
        reasoning_budget_tokens: thinking_off(body).then_some(0),
        draw,
        image_model,
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
