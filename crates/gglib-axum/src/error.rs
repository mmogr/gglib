//! Axum-specific error types and mappings.
//!
//! This module provides error types for the Axum adapter and mappings
//! from CoreError to HTTP status codes and response bodies.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use gglib_core::{CoreError, ProcessError, RepositoryError};
use serde::Serialize;
use thiserror::Error;

/// Axum-specific error type.
#[derive(Debug, Error)]
pub enum HttpError {
    /// Resource not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Bad request (invalid input).
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// Conflict (resource already exists).
    #[error("Conflict: {0}")]
    Conflict(String),

    /// Service unavailable (e.g., external service down).
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    /// Internal server error.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// JSON error response body.
#[derive(Serialize)]
struct ErrorBody {
    error: String,
    status: u16,
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            HttpError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            HttpError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            HttpError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            HttpError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
            HttpError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        let body = ErrorBody {
            error: message,
            status: status.as_u16(),
        };

        (status, axum::Json(body)).into_response()
    }
}

impl From<CoreError> for HttpError {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::Repository(repo_err) => repo_err.into(),
            CoreError::Process(proc_err) => proc_err.into(),
            CoreError::Validation(msg) => HttpError::BadRequest(msg),
            CoreError::Configuration(msg) => HttpError::Internal(format!("Config: {}", msg)),
            CoreError::ExternalService(msg) => HttpError::ServiceUnavailable(msg),
            CoreError::Internal(msg) => HttpError::Internal(msg),
        }
    }
}

impl From<RepositoryError> for HttpError {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::NotFound(msg) => HttpError::NotFound(msg),
            RepositoryError::AlreadyExists(msg) => HttpError::Conflict(msg),
            RepositoryError::Storage(msg) => HttpError::Internal(format!("Storage: {}", msg)),
            RepositoryError::Serialization(msg) => HttpError::Internal(format!("Serialization: {}", msg)),
            RepositoryError::Constraint(msg) => HttpError::BadRequest(msg),
        }
    }
}

impl From<ProcessError> for HttpError {
    fn from(err: ProcessError) -> Self {
        match err {
            ProcessError::NotRunning(msg) => HttpError::NotFound(msg),
            ProcessError::StartFailed(msg) => HttpError::ServiceUnavailable(msg),
            ProcessError::StopFailed(msg) => HttpError::Internal(format!("Stop failed: {}", msg)),
            ProcessError::HealthCheckFailed(msg) => HttpError::ServiceUnavailable(msg),
            ProcessError::Configuration(msg) => HttpError::BadRequest(msg),
            ProcessError::ResourceExhausted(msg) => HttpError::ServiceUnavailable(msg),
            ProcessError::Internal(msg) => HttpError::Internal(msg),
        }
    }
}
