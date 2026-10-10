//! `GET /v1/chats`, `GET /v1/chats/{id}` and `POST /v1/chats/{id}/changes`.
//!
//! Every error message here is fixed text or a [`HubChatsError`]'s, which is
//! fixed text too: nothing echoes a path, a title or a row.

use std::sync::Arc;

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use gglib_core::domain::branching::ChatChange;
use gglib_core::ports::{HubChatsError, HubChatsPort};
use serde_json::Value;

use crate::models::ErrorResponse;
use crate::server::AppState;

pub(super) type Answer = Result<Response, Response>;

fn refused(err: &HubChatsError) -> Response {
    let status =
        StatusCode::from_u16(err.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let error_type = if status.is_client_error() {
        "invalid_request_error"
    } else {
        "server_error"
    };
    (
        status,
        Json(ErrorResponse::with_code(
            err.to_string(),
            error_type,
            err.code(),
        )),
    )
        .into_response()
}

/// The answer when this proxy was started without the hub's chats.
pub(super) fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(ErrorResponse::with_code(
            "this proxy is not running under the gglib daemon, so it holds no chats",
            "service_unavailable",
            "chats_unavailable",
        )),
    )
        .into_response()
}

/// The chats, or `None` when this proxy was started without them.
pub(super) fn chats(state: &AppState) -> Option<Arc<dyn HubChatsPort>> {
    state.chats.clone()
}

/// `GET /v1/chats`: every chat, newest first.
pub(crate) async fn list_chats(State(state): State<AppState>) -> Answer {
    let list = chats(&state)
        .ok_or_else(unavailable)?
        .list()
        .await
        .map_err(|e| refused(&e))?;
    Ok(Json(list).into_response())
}

/// `GET /v1/chats/{id}`: one chat and its rows. An id that is not a number
/// names no chat.
pub(crate) async fn open_chat(State(state): State<AppState>, Path(id): Path<String>) -> Answer {
    let chats = chats(&state).ok_or_else(unavailable)?;
    let id = id
        .parse::<i64>()
        .map_err(|_| refused(&HubChatsError::NotFound))?;
    let open = chats.open(id).await.map_err(|e| refused(&e))?;
    Ok(Json(open).into_response())
}

/// `POST /v1/chats/{id}/changes`: an edit, a regenerate or a branch of one
/// chat, made as the hub's branching rules say (ADR 0017). The answer names
/// the chat to show, whether it is a new branch, and whether its last
/// question is now to be answered, which a turn that says `answer_saved`
/// does. A body that is no change is refused `invalid_request`.
pub(crate) async fn change_chat(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> Answer {
    let chats = chats(&state).ok_or_else(unavailable)?;
    let id = id
        .parse::<i64>()
        .map_err(|_| refused(&HubChatsError::NotFound))?;
    let Some(change) = body
        .ok()
        .and_then(|Json(body)| serde_json::from_value::<ChatChange>(body).ok())
    else {
        return Err(not_a_change());
    };
    let changed = chats.change(id, &change).await.map_err(|e| refused(&e))?;
    Ok(Json(changed).into_response())
}

/// The answer to a body that is no change.
fn not_a_change() -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::with_code(
            "a change's body is {\"kind\": \"edit\", \"message_id\": <number>, \"content\": <text>, \"images\": [<id>]}, or {\"kind\": \"regenerate\" or \"branch\", \"message_id\": <number>}",
            "invalid_request_error",
            "invalid_request",
        )),
    )
        .into_response()
}
