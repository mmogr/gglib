//! Axum-specific error types and mappings.
//!
//! This module provides error types for the Axum adapter and mappings
//! from `CoreError` and `GuiError` to HTTP status codes and response bodies.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use gglib_app_services::GuiError;
use gglib_core::ports::chat_history::ChatHistoryError;
use gglib_core::ports::{AttachmentError, RunsError};
use gglib_core::services::ChangeError;
use gglib_core::{CoreError, RepositoryError};
use serde::Serialize;
use thiserror::Error;

/// The `type` of [`HttpError::EndpointDiffers`]'s body.
pub const ENDPOINT_DIFFERS: &str = gglib_core::contracts::http::daemon::ENDPOINT_DIFFERS_TYPE;

/// Axum-specific error type.
#[derive(Debug, Error)]
pub enum HttpError {
    /// Resource not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Bad request (invalid input).
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// Refused whoever asks (403).
    #[error("Forbidden: {0}")]
    Forbidden(String),

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
        suggested_command: String,
        reason: String,
    },

    /// Too many concurrent requests (429).
    #[error("Too many requests: {0}")]
    TooManyRequests(String),

    /// Internal server error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// A start that found the endpoint on otherwise, and left it alone
    /// (409, type [`ENDPOINT_DIFFERS`]); the body's `metadata` is the
    /// [`EndpointChange`](gglib_app_services::EndpointChange), for a caller
    /// that asks the user before it starts again with `replace`.
    #[error("the endpoint is {}: {}", .0.now, .0.action)]
    EndpointDiffers(gglib_app_services::EndpointChange),

    /// A refusal with a stable code, which the body carries as `type`.
    #[error("{message}")]
    Coded {
        status: StatusCode,
        code: &'static str,
        message: String,
    },
}

/// JSON error response body.
///
/// The one shape every route can return, so it is exported alongside the
/// success bodies rather than left for clients to guess at.
#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
struct ErrorBody {
    error: String,
    status: u16,
    /// Stable error type discriminant for client-side handling
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    error_type: Option<String>,
    /// Optional additional metadata for specific error types
    #[cfg_attr(feature = "ts-bindings", ts(type = "unknown", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, message, error_type, metadata) = match &self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone(), None, None),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone(), None, None),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone(), None, None),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg.clone(), None, None),
            Self::ServiceUnavailable(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, msg.clone(), None, None)
            }
            Self::LlamaServerNotInstalled {
                message,
                expected_path,
                suggested_command,
                reason,
            } => {
                let metadata_json = serde_json::json!({
                    "expectedPath": expected_path,
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
            Self::TooManyRequests(msg) => (StatusCode::TOO_MANY_REQUESTS, msg.clone(), None, None),
            Self::EndpointDiffers(change) => (
                StatusCode::CONFLICT,
                format!(
                    "The endpoint is {}, so it was left as it is: switching it to {} would \
                     cut off the clients connected now.",
                    change.now, change.action
                ),
                Some(ENDPOINT_DIFFERS.to_owned()),
                serde_json::to_value(change).ok(),
            ),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone(), None, None),
            Self::Coded {
                status,
                code,
                message,
            } => (*status, message.clone(), Some((*code).to_owned()), None),
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
            CoreError::Settings(settings_err) => Self::BadRequest(settings_err.to_string()),
            CoreError::Validation(msg) => Self::BadRequest(msg),
        }
    }
}

impl From<RepositoryError> for HttpError {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::NotFound(msg) => Self::NotFound(msg),
            RepositoryError::AlreadyExists(msg) => Self::Conflict(msg),
            RepositoryError::Storage(msg) => Self::Internal(format!("Storage: {msg}")),
            RepositoryError::Serialization(msg) => Self::Internal(format!("Serialization: {msg}")),
            RepositoryError::Constraint(msg) => Self::BadRequest(msg),
        }
    }
}

impl From<GuiError> for HttpError {
    fn from(e: GuiError) -> Self {
        // Coarse mapping - refine later as needed
        match e {
            GuiError::NotFound { entity, id } => {
                Self::NotFound(format!("{entity} with id {id} not found"))
            }
            GuiError::ValidationFailed(msg) => Self::BadRequest(msg),
            GuiError::Conflict(msg) => Self::Conflict(msg),
            GuiError::Unavailable(msg) => Self::ServiceUnavailable(msg),
            GuiError::LlamaServerNotInstalled {
                expected_path,
                suggested_command,
                reason,
            } => Self::LlamaServerNotInstalled {
                message: format!("llama-server not installed ({reason}). Run: {suggested_command}"),
                expected_path,
                suggested_command,
                reason,
            },
            // The code is `LoadFailure::CODE`, spelled out for the code scan.
            GuiError::LoadFailed(message) => Self::Coded {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "model_load_failed",
                message,
            },
            GuiError::Internal(msg) => Self::Internal(msg),
        }
    }
}

impl From<RunsError> for HttpError {
    fn from(e: RunsError) -> Self {
        Self::Coded {
            status: StatusCode::from_u16(e.http_status())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            code: e.code(),
            message: e.to_string(),
        }
    }
}

impl From<ChatHistoryError> for HttpError {
    fn from(err: ChatHistoryError) -> Self {
        match err {
            ChatHistoryError::ConversationNotFound(id) => {
                Self::NotFound(format!("Conversation not found: {id}"))
            }
            ChatHistoryError::MessageNotFound(id) => {
                Self::NotFound(format!("Message not found: {id}"))
            }
            ChatHistoryError::InvalidRole(role) => {
                Self::BadRequest(format!("Invalid message role: {role}"))
            }
            ChatHistoryError::Database(msg) => Self::Internal(format!("Database error: {msg}")),
            ChatHistoryError::UnreadableSettings(_) => Self::Conflict(err.to_string()),
            ChatHistoryError::Attachment(refusal) => refusal.into(),
        }
    }
}

impl From<ChangeError> for HttpError {
    fn from(err: ChangeError) -> Self {
        let coded = |status: u16, code, message: String| Self::Coded {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            code,
            message,
        };
        match err {
            ChangeError::Refused(refused) => {
                coded(refused.http_status(), refused.code(), refused.to_string())
            }
            ChangeError::History(ChatHistoryError::ConversationNotFound(id)) => coded(
                404,
                "conversation_not_found",
                format!("no conversation has id {id}"),
            ),
            ChangeError::History(ChatHistoryError::MessageNotFound(id)) => coded(
                404,
                "message_not_found",
                format!("message {id} is not in this conversation"),
            ),
            ChangeError::History(other) => other.into(),
        }
    }
}

impl From<AttachmentError> for HttpError {
    fn from(err: AttachmentError) -> Self {
        err.code().map_or_else(
            || Self::Internal(err.to_string()),
            |code| Self::Coded {
                status: StatusCode::from_u16(err.http_status())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                code,
                message: err.to_string(),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The last link of the duplicate-model chain. `AlreadyExists` is raised
    /// by `ModelService::import_from_file`; this pins the other end, so that a
    /// rearrangement of these `From` impls cannot quietly send a duplicate
    /// back as a 500.
    ///
    /// All three routes in. The `POST /api/models` path is the `GuiError` one
    /// — `ModelOps::add` catches the duplicate and re-raises it as
    /// `GuiError::Conflict`, so the blanket `CoreError` conversion below never
    /// sees it on that route. Pinning only the other two would leave the arm
    /// the product actually uses untested.
    #[test]
    fn already_exists_reaches_the_client_as_409() {
        let direct: HttpError = RepositoryError::AlreadyExists("dup".to_string()).into();
        assert_eq!(direct.into_response().status(), StatusCode::CONFLICT);

        let wrapped: HttpError =
            CoreError::Repository(RepositoryError::AlreadyExists("dup".to_string())).into();
        assert_eq!(wrapped.into_response().status(), StatusCode::CONFLICT);

        let from_gui: HttpError = GuiError::Conflict("dup".to_string()).into();
        assert_eq!(from_gui.into_response().status(), StatusCode::CONFLICT);
    }

    /// A failed load reaches the app as its own sentence, coded, with no
    /// wrapping (#1959 C5): the inspector places `error` as it stands.
    #[test]
    fn a_failed_load_reaches_the_client_coded_in_its_own_words() {
        let said = "llama-server stopped while loading qwen: it exited with status 1";
        let HttpError::Coded {
            status,
            code,
            message,
        } = GuiError::LoadFailed(said.to_owned()).into()
        else {
            panic!("a failed load is a coded refusal");
        };
        assert_eq!(
            (status, code, message.as_str()),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                gglib_core::ports::LoadFailure::CODE,
                said
            )
        );
    }

    /// The neighbouring repository errors must not also become 409 — a
    /// conflict has to mean something.
    #[test]
    fn other_repository_errors_keep_their_own_status() {
        let not_found: HttpError = RepositoryError::NotFound("gone".to_string()).into();
        assert_eq!(not_found.into_response().status(), StatusCode::NOT_FOUND);

        let storage: HttpError = RepositoryError::Storage("disk".to_string()).into();
        assert_eq!(
            storage.into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    /// An image refusal keeps its code and status on the way out, alone or
    /// from a save that named an image the store lacks.
    #[test]
    fn an_attachment_refusal_reaches_the_client_by_its_code() {
        use gglib_core::domain::AttachmentId;

        let id = AttachmentId::of(b"never uploaded");
        let cases = [
            (AttachmentError::TooLarge, 413, "image_too_large"),
            (AttachmentError::Unsupported, 400, "unsupported_image"),
            (
                AttachmentError::NotFound(id.clone()),
                400,
                "attachment_not_found",
            ),
            (
                AttachmentError::RequestTooLarge,
                400,
                "request_images_too_large",
            ),
        ];
        for (refusal, want_status, want_code) in cases {
            let HttpError::Coded { status, code, .. } = refusal.into() else {
                panic!("{want_code} is a coded refusal");
            };
            assert_eq!((status.as_u16(), code), (want_status, want_code));
        }

        let from_save: HttpError =
            ChatHistoryError::Attachment(AttachmentError::NotFound(id)).into();
        assert!(matches!(
            from_save,
            HttpError::Coded {
                status: StatusCode::BAD_REQUEST,
                code: "attachment_not_found",
                ..
            }
        ));

        let storage: HttpError = AttachmentError::Storage("disk".to_string()).into();
        assert!(matches!(storage, HttpError::Internal(_)));
    }

    /// A start left alone because the endpoint is on otherwise is a 409 a
    /// person reads, wherever it lands (the tray's Turn on among them), with
    /// the change beside it for a caller that asks before switching.
    #[tokio::test]
    async fn a_start_left_alone_says_why_in_words_a_person_reads() {
        let refused = HttpError::EndpointDiffers(gglib_app_services::EndpointChange {
            now: "on at port 8080, serving only Qwen3-4B".to_owned(),
            action: "serve every model again".to_owned(),
            moves: false,
        })
        .into_response();
        assert_eq!(refused.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(refused.into_body(), usize::MAX)
            .await
            .expect("the body");
        let body: serde_json::Value = serde_json::from_slice(&body).expect("JSON");

        assert_eq!(
            body["error"],
            "The endpoint is on at port 8080, serving only Qwen3-4B, so it was left as it is: \
             switching it to serve every model again would cut off the clients connected now."
        );
        assert_eq!(body["type"], ENDPOINT_DIFFERS);
        assert_eq!(body["metadata"]["action"], "serve every model again");
    }
}
