//! The size a request body may be on the routes that take images, and the
//! refusal of one over it.
//!
//! An image rides in the body as base64, so `POST /v1/chat/completions` and
//! `PUT /v1/runs/{id}` take [`MAX_BODY_BYTES`] in place of axum's 2 MiB
//! default. A body over it is refused with a code, as every other refusal
//! here is: left to the extractor it is a line of plain text on the first
//! route, and on the second it reads as a body that is not JSON.

use axum::Json;
use axum::extract::DefaultBodyLimit;
use axum::extract::rejection::BytesRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use gglib_core::contracts::http::MAX_BODY_BYTES;

use crate::models::ErrorResponse;

/// The limit, as the layer a route takes it by.
pub(crate) fn layer() -> DefaultBodyLimit {
    DefaultBodyLimit::max(MAX_BODY_BYTES)
}

/// The refusal of a body over the limit, when `status` (a body extractor's
/// rejection's) says that is what it was; `None` for any other rejection.
pub(crate) fn too_large(status: StatusCode) -> Option<Response> {
    (status == StatusCode::PAYLOAD_TOO_LARGE).then(|| {
        let message = format!(
            "The request body is over the {} MiB limit.",
            MAX_BODY_BYTES / (1024 * 1024)
        );
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ErrorResponse::with_code(
                message,
                "invalid_request_error",
                "request_too_large",
            )),
        )
            .into_response()
    })
}

/// The answer to a body that could not be read: the coded refusal when it
/// was over the limit, and the extractor's own otherwise.
pub(crate) fn rejected(rejection: BytesRejection) -> Response {
    too_large(rejection.status()).unwrap_or_else(|| rejection.into_response())
}
