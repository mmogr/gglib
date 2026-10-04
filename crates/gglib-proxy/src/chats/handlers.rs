//! `GET /v1/chats` and `GET /v1/chats/{id}`.
//!
//! Every error message here is fixed text or a [`HubChatsError`]'s, which is
//! fixed text too: nothing echoes a path, a title or a row.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use gglib_core::ports::{HubChatsError, HubChatsPort};

use crate::models::ErrorResponse;
use crate::server::AppState;

pub(super) type Answer = Result<Response, Response>;

fn refused(err: &HubChatsError) -> Response {
    let status =
        StatusCode::from_u16(err.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let error_type = if status == StatusCode::NOT_FOUND {
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
