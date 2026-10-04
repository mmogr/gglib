//! `PUT /v1/runs/{id}?kind=agent`: a paired device adds a turn to one of the
//! hub's chats, and the hub runs the reply as an agent run in the device's
//! scope, saved to the chat.
//!
//! The body is `{conversation_id, content, images}` and nothing more, with
//! `images` (stored image ids) left out of a text-only turn: the hub
//! rebuilds the history from its own record. Only a named device may: a
//! local client writes to its chats at `/api`. Every error message is fixed
//! text or the starter's, which is fixed text too.

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
        "A turn on the hub's chats comes from a paired device, through the tunnel.".to_owned(),
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
            "a turn's body is {\"conversation_id\": <number>, \"content\": <text>, \"images\": [<id>]}",
        ));
    };
    let created = turns.start(device, id, turn).await.map_err(refused)?;
    let status = if created.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(created.info)).into_response())
}
