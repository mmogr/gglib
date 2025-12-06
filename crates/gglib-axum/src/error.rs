//! Axum-specific error types and mappings.
//!
//! This module provides error types for the Axum adapter and mappings
//! from CoreError to HTTP status codes and response bodies.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

/// Axum-specific error type.
#[derive(Debug, Error)]
pub enum HttpError {
    /// Core domain error.
    #[error("{0}")]
    Core(String),

    /// Resource not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Bad request (invalid input).
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// Internal server error.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            HttpError::Core(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            HttpError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            HttpError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            HttpError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        let body = serde_json::json!({
            "error": message,
            "status": status.as_u16(),
        });

        (status, axum::Json(body)).into_response()
    }
}

// Placeholder: impl From<CoreError> for HttpError will be added during extraction
