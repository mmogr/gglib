//! The one shape every refusal on the proxy's router takes: `OpenAI`'s
//! `{"error": {"message", "type", "code"}}`, with a code from the table in
//! `error_codes` (published as `docs/error-codes.json`).
//!
//! Handlers write their refusals through [`ErrorResponse`]. What is here is
//! the shared way to write one, and the answers no handler writes: a path no
//! route serves, a method a route does not take, and a request an extractor
//! could not read. Left to axum, each of those is plain text or an empty body.
//!
//! The MCP gateway at `/mcp` is not answered from here: it speaks JSON-RPC
//! and MCP's own transport statuses.

use axum::Json;
use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use axum::http::{Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

use crate::models::ErrorResponse;

/// A refusal with `status`, and `code` beside `error_type`.
pub(crate) fn error(
    status: StatusCode,
    error_type: &str,
    code: &str,
    message: impl Into<String>,
) -> Response {
    (
        status,
        Json(ErrorResponse::with_code(message, error_type, code)),
    )
        .into_response()
}

/// 400 `invalid_request`: a request the route could not read, or one that is
/// not valid.
pub(crate) fn invalid(message: impl Into<String>) -> Response {
    error(
        StatusCode::BAD_REQUEST,
        "invalid_request_error",
        "invalid_request",
        message,
    )
}

/// 500 `internal_error`: gglib failed on its own side.
pub(crate) fn internal(message: impl Into<String>) -> Response {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "server_error",
        "internal_error",
        message,
    )
}

/// The refusal of a request an extractor could not read, from its
/// rejection's status and words: 413 `request_too_large` for a body over its
/// route's limit, 500 `internal_error` for a rejection that is gglib's fault,
/// and 400 `invalid_request` for the rest (a body that is not JSON, or not
/// sent as JSON, or lacks a field; a query or a path segment that does not
/// read).
pub(crate) fn unreadable(status: StatusCode, words: String) -> Response {
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        error(status, "invalid_request_error", "request_too_large", words)
    } else if status.is_server_error() {
        internal(words)
    } else {
        invalid(words)
    }
}

/// A route's one path parameter, read as `Path<String>` reads it, with a
/// segment that does not read (one that is not UTF-8 once decoded) refused
/// by [`unreadable`].
pub(crate) struct Segment(pub(crate) String);

impl<S: Send + Sync> FromRequestParts<S> for Segment {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Path::<String>::from_request_parts(parts, state)
            .await
            .map(|Path(segment)| Self(segment))
            .map_err(|rejection| unreadable(rejection.status(), rejection.body_text()))
    }
}

/// The router's fallback: 404 `unknown_route` for a path no route serves.
pub(crate) async fn unknown_route(uri: Uri) -> Response {
    error(
        StatusCode::NOT_FOUND,
        "invalid_request_error",
        "unknown_route",
        format!(
            "No route here serves {}. gglib's own routes are under /v1/gglib/.",
            uri.path()
        ),
    )
}

/// The answer to a method a route does not take: 405 `method_not_allowed`.
/// axum still writes the `Allow` header beside it.
pub(crate) async fn method_not_allowed(method: Method, uri: Uri) -> Response {
    error(
        StatusCode::METHOD_NOT_ALLOWED,
        "invalid_request_error",
        "method_not_allowed",
        format!("{} does not take {method}.", uri.path()),
    )
}
