//! `PUT /v1/runs/{id}?kind=agent`: a paired device adds a turn to one of the
//! hub's chats, and the hub runs the reply as an agent run in the device's
//! scope, saved to the chat.
//!
//! The body is `{conversation_id, content, images, thinking, answer_saved,
//! draw}` and nothing more, with `images` (stored image ids) left out of a
//! text-only turn, `thinking` (`"off"` or `"default"`) left out of one that
//! does not change the chat's Thinking choice, `answer_saved` left out of
//! one that adds a message, and `draw` sent only for a message sent with
//! Draw pressed: the hub rebuilds the history from its own record. A turn
//! that says `answer_saved: true` has empty `content` and no image, and
//! answers the question the chat already ends in (ADR 0017).
//! `?kind=chat&tools=3,7` is the other thing the starter runs: a chat the
//! device keeps itself, through the agent loop, offered the tools of those
//! of the named MCP servers that are open to paired devices and, with
//! `draw`, the image tool; `?kind=chat&tools=none` runs it offered none of
//! those servers' tools, and with `draw` the image tool alone.
//! Only a named device may: a local client writes to its chats at
//! `/api`. Every error message is fixed text or the starter's, which is
//! fixed text too.

use std::collections::BTreeSet;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::ports::{RunScope, TurnRefused};
use serde_json::Value;

use super::handlers::{Answer, error, invalid, unavailable};
use crate::server::AppState;

/// A turn from anything but a named device.
fn not_a_device() -> Response {
    error(
        StatusCode::FORBIDDEN,
        "invalid_request_error",
        "device_not_named",
        "A turn on the hub's chats, and a chat run with builtins, come from a paired device, \
         through the tunnel."
            .to_owned(),
    )
}

fn refused(refusal: TurnRefused) -> Response {
    let status = StatusCode::from_u16(refusal.status).unwrap_or(StatusCode::BAD_REQUEST);
    let error_type = if status == StatusCode::TOO_MANY_REQUESTS {
        "rate_limit_error"
    } else if status.is_server_error() {
        "server_error"
    } else {
        "invalid_request_error"
    };
    error(status, error_type, &refusal.code, refusal.message)
}

/// Start the device's turn, or answer with its run that has the id. 201
/// new, 200 existing.
pub(super) async fn put(state: &AppState, scope: RunScope, id: &str, body: Value) -> Answer {
    let Some(device) = scope.device() else {
        return Err(not_a_device());
    };
    let turns = state.turns.clone().ok_or_else(unavailable)?;
    let Ok(turn) = serde_json::from_value::<HubTurn>(body) else {
        return Err(invalid(
            "a turn's body is {\"conversation_id\": <number>, \"content\": <text>, \"images\": [<id>], \"thinking\": \"off\" or \"default\", \"answer_saved\": true or false, \"draw\": true, \"image_model\": <id>}",
        ));
    };
    let started = turns.start(device, id, turn).await;
    started.map(made).map_err(refused)
}

/// `PUT /v1/runs/{id}?kind=chat&tools=<id>,<id>[&draw=true[&image_model=<id>]]`: a chat the
/// device keeps itself, run through the agent loop with the tools of the
/// MCP servers open to paired devices whose ids `servers` holds, and the image
/// tool with `draw`. With `tools=none` (no server), none of those servers'
/// tools: the image tool alone with `draw`, and without it a run that calls
/// no tool.
/// The body is the device's unchanged `OpenAI` chat request. Only a named
/// device may, as for a turn: the loop's slot, its model's hold, the MCP
/// servers and the image tool are the daemon's, reached through the same
/// starter.
pub(super) async fn put_chat(
    state: &AppState,
    scope: RunScope,
    id: &str,
    body: Value,
    (draw, image_model): (bool, Option<u32>),
    servers: BTreeSet<i64>,
) -> Answer {
    let Some(device) = scope.device() else {
        return Err(not_a_device());
    };
    let turns = state.turns.clone().ok_or_else(unavailable)?;
    let started = turns
        .start_chat(device, id, body, draw, image_model, servers)
        .await;
    started.map(made).map_err(refused)
}

/// The run a starter made or found: 201 new, 200 existing.
fn made(created: gglib_core::ports::Created) -> Response {
    let status = if created.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    (status, Json(created.info)).into_response()
}
