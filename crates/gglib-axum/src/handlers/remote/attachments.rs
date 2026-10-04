//! The far machine's stored images, for this machine's chat page: an image
//! sent to it ahead of a turn on one of its chats, and one read back to
//! show. Each is forwarded through the tunnel with the stored key, as the
//! chats are, and nothing is kept here: the image is the far machine's.

use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::Response;
use gglib_app_services::FarProxy;
use gglib_core::contracts::http::attachments::PAIRED_ATTACHMENT_CACHE_CONTROL;
use gglib_core::domain::AttachmentId;
use gglib_core::request_pipeline::{JPEG_MIME, PNG_MIME};

use super::chats::relay;
use crate::error::HttpError;
use crate::handlers::attachments::{kept_as_typed, no_such_image, uploaded};
use crate::state::AppState;

/// `POST /api/remote/attachments`. A body over one image is refused here,
/// by the code the far machine would refuse it with, and is not sent.
pub(crate) async fn upload_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, HttpError> {
    let bytes = uploaded(body)?;
    let far = state.remote.far().await?;
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok());
    upload_via(&far, bytes, content_type).await
}

/// `GET /api/remote/attachments/{id}`.
pub(crate) async fn fetch_attachment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, HttpError> {
    let far = state.remote.far().await?;
    fetch_via(&far, &id).await
}

pub(super) async fn upload_via(
    far: &FarProxy,
    bytes: Bytes,
    content_type: Option<&str>,
) -> Result<Response, HttpError> {
    Ok(relay(far.upload_attachment(bytes, content_type).await?).await)
}

/// A path that is not an id is refused here, and nothing is sent: only an
/// id is ever put in the far machine's path.
///
/// What comes back is served from this daemon's origin, so the far machine
/// does not get to say what it is: the bytes are an image of a type the
/// store keeps, or bytes of no type at all, and a browser may not guess.
pub(super) async fn fetch_via(far: &FarProxy, id: &str) -> Result<Response, HttpError> {
    let id = AttachmentId::parse(id).map_err(|_| no_such_image())?;
    let mut response = relay(far.fetch_attachment(&id).await?).await;
    if response.status().is_success() {
        let headers = response.headers_mut();
        let stored_type = headers
            .get(header::CONTENT_TYPE)
            .is_some_and(|sent| sent == PNG_MIME || sent == JPEG_MIME);
        if !stored_type {
            let untyped = HeaderValue::from_static("application/octet-stream");
            headers.insert(header::CONTENT_TYPE, untyped);
        }
        kept_as_typed(headers, PAIRED_ATTACHMENT_CACHE_CONTROL);
    }
    Ok(response)
}

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod attachments_tests;
