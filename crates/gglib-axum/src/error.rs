//! Axum-specific error types and mappings.
//!
//! This module provides error types for the Axum adapter and mappings
//! from CoreError and GuiError to HTTP status codes and response bodies.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use gglib_core::ports::chat_history::ChatHistoryError;
use gglib_core::{CoreError, ProcessError, RepositoryError};
use gglib_gui::GuiError;
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

    /// llama-server not installed - special case with metadata.
    #[error("llama-server not installed: {message}")]
    LlamaServerNotInstalled {
        message: String,
        expected_path: String,
        legacy_path: Option<String>,
        suggested_command: String,
        reason: String,
    },

    /// Internal server error.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// JSON error response body.
#[derive(Serialize)]
struct ErrorBody {
    error: String,
    status: u16,
    /// Stable error type discriminant for client-side handling
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    error_type: Option<String>,
    /// Optional additional metadata for specific error types
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, message, error_type, metadata) = match &self {
            HttpError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone(), None, None),
            HttpError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone(), None, None),
            HttpError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone(), None, None),
            HttpError::ServiceUnavailable(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, msg.clone(), None, None)
            }
            HttpError::LlamaServerNotInstalled {
                message,
                expected_path,
                legacy_path,
                suggested_command,
                reason,
            } => {
                let metadata_json = serde_json::json!({
                    "expectedPath": expected_path,
                    "legacyPath": legacy_path,
                    "suggestedCommand": suggested_command,
                    "reason": reason,
                });
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    message.clone(),
                    Some("LLAMA_SERVER_NOT_INSTALLED".to_string()),
                    Some(metadata_json),
                )
            }
            HttpError::Internal(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg.clone(), None, None)
            }
        };

        let body = ErrorBody {
            error: message,
            status: status.as_u16(),
            error_type,
            metadata,
        };

        (status, axum::Json(body)).into_response()
    }
}

impl From<CoreError> for HttpError {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::Repository(repo_err) => repo_err.into(),
            CoreError::Process(proc_err) => proc_err.into(),
            CoreError::Settings(settings_err) => HttpError::BadRequest(settings_err.to_string()),
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
            RepositoryError::Serialization(msg) => {
                HttpError::Internal(format!("Serialization: {}", msg))
            }
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

impl From<GuiError> for HttpError {
    fn from(e: GuiError) -> Self {
        // Coarse mapping - refine later as needed
        match e {
            GuiError::NotFound { entity, id } => {
                HttpError::NotFound(format!("{} with id {} not found", entity, id))
            }
            GuiError::ValidationFailed(msg) => HttpError::BadRequest(msg),
            GuiError::Conflict(msg) => HttpError::Conflict(msg),
            GuiError::Unavailable(msg) => HttpError::ServiceUnavailable(msg),
            GuiError::LlamaServerNotInstalled {
                expected_path,
                legacy_path,
                suggested_command,
                reason,
            } => HttpError::LlamaServerNotInstalled {
                message: format!(
                    "llama-server not installed ({}). Run: {}",
                    reason, suggested_command
                ),
                expected_path,
                legacy_path,
                suggested_command,
                reason,
            },
            GuiError::Internal(msg) => HttpError::Internal(msg),
        }
    }
}

impl From<ChatHistoryError> for HttpError {
    fn from(err: ChatHistoryError) -> Self {
        match err {
            ChatHistoryError::ConversationNotFound(id) => {
                HttpError::NotFound(format!("Conversation not found: {}", id))
            }
            ChatHistoryError::MessageNotFound(id) => {
                HttpError::NotFound(format!("Message not found: {}", id))
            }
            ChatHistoryError::InvalidRole(role) => {
                HttpError::BadRequest(format!("Invalid message role: {}", role))
            }
            ChatHistoryError::Database(msg) => {
                HttpError::Internal(format!("Database error: {}", msg))
            }
        }
    }
}
