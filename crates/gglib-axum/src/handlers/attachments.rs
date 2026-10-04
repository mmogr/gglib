//! `POST /api/attachments` and `GET /api/attachments/{id}`: an image the
//! chat page sends ahead of the message that names it, and one it shows.
//!
//! The body of an upload is the file itself. What counts as an image and
//! how large one may be is core's one ingest
//! ([`gglib_core::services::AttachmentService`]); the limit on the body is
//! the same number, so an oversized file is refused while it arrives.
//! Nothing here logs an image.

use axum::Json;
use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use gglib_core::contracts::http::attachments::{ATTACHMENT_CACHE_CONTROL, NOT_AN_ATTACHMENT_ID};
use gglib_core::domain::{AttachmentId, AttachmentUpload};
use gglib_core::ports::AttachmentError;
use gglib_core::request_pipeline::MAX_IMAGE_BYTES;

use crate::error::HttpError;
use crate::state::AppState;

/// Mark `headers` as those of a stored image read by its id: cached as
/// `cache_control` says, and never read as any type but the one
/// `Content-Type` names.
pub(crate) fn kept_as_typed(headers: &mut HeaderMap, cache_control: &'static str) {
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
}

/// The most an upload's body may be: one image.
pub(crate) fn body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(MAX_IMAGE_BYTES)
}

/// An upload's body, or the refusal of one that could not be read: over the
/// limit is the coded 413 of an image too large, not the extractor's text.
pub(crate) fn uploaded(body: Result<Bytes, BytesRejection>) -> Result<Bytes, HttpError> {
    body.map_err(|rejection| {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            AttachmentError::TooLarge.into()
        } else {
            HttpError::BadRequest(rejection.body_text())
        }
    })
}

/// The refusal of a path that is not an id: it names no image.
pub(crate) fn no_such_image() -> HttpError {
    HttpError::Coded {
        status: StatusCode::NOT_FOUND,
        code: "attachment_not_found",
        message: NOT_AN_ATTACHMENT_ID.to_owned(),
    }
}

/// A refusal of a read by id: as any other, but an id the store lacks is
/// the 404 of a path that names nothing.
fn unfetched(err: AttachmentError) -> HttpError {
    let status =
        StatusCode::from_u16(err.fetch_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    match HttpError::from(err) {
        HttpError::Coded { code, message, .. } => HttpError::Coded {
            status,
            code,
            message,
        },
        other => other,
    }
}

/// `POST /api/attachments`: store the body as an image, and answer its id,
/// its type and size, and the tokens it is estimated to cost.
pub(crate) async fn upload(
    State(state): State<AppState>,
    body: Result<Bytes, BytesRejection>,
) -> Result<Json<AttachmentUpload>, HttpError> {
    let bytes = uploaded(body)?;
    Ok(Json(state.core.attachments().ingest(&bytes).await?))
}

/// `GET /api/attachments/{id}`: the image's bytes, as they were sent, with
/// the type its first bytes gave it.
pub(crate) async fn fetch(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, HttpError> {
    let id = AttachmentId::parse(&id).map_err(|_| no_such_image())?;
    let blob = state
        .core
        .attachments()
        .blob(&id)
        .await
        .map_err(unfetched)?;
    let mut response = blob.data.into_response();
    let headers = response.headers_mut();
    if let Ok(mime) = HeaderValue::from_str(&blob.mime) {
        headers.insert(header::CONTENT_TYPE, mime);
    }
    kept_as_typed(headers, ATTACHMENT_CACHE_CONTROL);
    Ok(response)
}
