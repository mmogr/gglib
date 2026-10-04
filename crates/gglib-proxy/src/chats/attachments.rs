//! `POST /v1/attachments` and `GET /v1/attachments/{id}`: an image a paired
//! device sends ahead of the turn that names it, and one it reads back for
//! a chat it has opened.
//!
//! The body of an upload is the file itself. What counts as an image and
//! how large one may be is the hub's one ingest, behind
//! [`HubChatsPort::attach`]; the limit on the body is the same number, so an
//! oversized file is refused while it arrives. Nothing here logs an image
//! or echoes a path: an error names at most an id.
//!
//! [`HubChatsPort::attach`]: gglib_core::ports::HubChatsPort::attach

use axum::Json;
use axum::extract::rejection::BytesRejection;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use gglib_core::contracts::http::attachments::{
    NOT_AN_ATTACHMENT_ID, PAIRED_ATTACHMENT_CACHE_CONTROL,
};
use gglib_core::domain::AttachmentId;
use gglib_core::ports::AttachmentError;
use gglib_core::request_pipeline::MAX_IMAGE_BYTES;

use super::handlers::{Answer, chats, unavailable};
use crate::models::ErrorResponse;
use crate::server::AppState;

/// The most an upload's body may be: one image.
pub(crate) fn body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(MAX_IMAGE_BYTES)
}

/// A refusal in the proxy's error shape, answered with `status`. A failure
/// of the store is fixed text: its own words are logged, not sent.
fn refused(err: &AttachmentError, status: u16) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = err.code().map_or_else(
        || {
            tracing::warn!(error = %err, "the hub's image store failed");
            ErrorResponse::with_code(
                "the hub's image store failed",
                "server_error",
                "internal_error",
            )
        },
        |code| ErrorResponse::with_code(err.to_string(), "invalid_request_error", code),
    );
    (status, Json(body)).into_response()
}

/// `POST /v1/attachments`: store the body as an image, and answer its id,
/// its type and size, and the tokens it is estimated to cost.
pub(crate) async fn upload_attachment(
    State(state): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Answer {
    let chats = chats(&state).ok_or_else(unavailable)?;
    let bytes = body.map_err(|rejection| {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            let too_large = AttachmentError::TooLarge;
            refused(&too_large, too_large.http_status())
        } else {
            rejection.into_response()
        }
    })?;
    let stored = chats
        .attach(&bytes)
        .await
        .map_err(|e| refused(&e, e.http_status()))?;
    Ok(Json(stored).into_response())
}

/// `GET /v1/attachments/{id}`: the image's bytes, as they were sent, with
/// the type its first bytes gave it, and not to be stored. A path that is
/// not an id names no image.
pub(crate) async fn fetch_attachment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Answer {
    let chats = chats(&state).ok_or_else(unavailable)?;
    let id = AttachmentId::parse(&id).map_err(|_| {
        let body = ErrorResponse::with_code(
            NOT_AN_ATTACHMENT_ID,
            "invalid_request_error",
            "attachment_not_found",
        );
        (StatusCode::NOT_FOUND, Json(body)).into_response()
    })?;
    let blob = chats
        .attachment(&id)
        .await
        .map_err(|e| refused(&e, e.fetch_status()))?;
    let mut response = blob.data.into_response();
    let headers = response.headers_mut();
    if let Ok(mime) = HeaderValue::from_str(&blob.mime) {
        headers.insert(header::CONTENT_TYPE, mime);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PAIRED_ATTACHMENT_CACHE_CONTROL),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}
