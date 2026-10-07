//! Semantic error types for GUI operations.
//!
//! These errors are domain-focused, not HTTP-focused. Adapters map
//! `GuiError` to their specific error types (`TauriError`, `HttpError`).

use std::fmt;

use gglib_core::paths::{LLAMA_INSTALL_COMMAND, llama_server_path};

/// Semantic errors for GUI backend operations.
///
/// Each variant represents a logical error condition that adapters
/// can map to appropriate responses (HTTP status codes, Tauri errors, etc.).
#[derive(Debug, Clone)]
pub enum GuiError {
    /// Entity not found (404-ish).
    NotFound {
        /// Type of entity (e.g., "model", "server", "download").
        entity: &'static str,
        /// Identifier that was not found.
        id: String,
    },

    /// Request validation failed (400-ish).
    ValidationFailed(String),

    /// Operation conflicts with current state (409-ish).
    Conflict(String),

    /// Service is temporarily unavailable (503-ish).
    Unavailable(String),

    /// llama-server binary is not installed or not accessible.
    ///
    /// This is a specific, actionable error that the GUI/Web UI can handle
    /// by displaying an installation prompt or migration dialog.
    LlamaServerNotInstalled {
        /// The path where llama-server was expected
        expected_path: String,
        /// Suggested command to fix the issue
        suggested_command: String,
        /// Reason for the failure (`NotFound`, `NotExecutable`, `PermissionDenied`)
        reason: String,
    },

    /// Unexpected internal error - should be refined over time.
    Internal(String),
}

impl fmt::Display for GuiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { entity, id } => write!(f, "{entity} not found: {id}"),
            Self::ValidationFailed(msg) => write!(f, "validation failed: {msg}"),
            Self::Conflict(msg) => write!(f, "conflict: {msg}"),
            Self::Unavailable(msg) => write!(f, "service unavailable: {msg}"),
            Self::LlamaServerNotInstalled {
                expected_path,
                suggested_command,
                reason,
            } => {
                write!(
                    f,
                    "llama-server not installed: {reason} at {expected_path}\nRun: {suggested_command}"
                )
            }
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for GuiError {}

// ============================================================================
// Conversions from core errors
// ============================================================================

impl From<gglib_core::download::DownloadError> for GuiError {
    fn from(err: gglib_core::download::DownloadError) -> Self {
        use gglib_core::download::DownloadError;
        match err {
            DownloadError::NotFound { message } => Self::NotFound {
                entity: "download",
                id: message,
            },
            DownloadError::NotInQueue { id } => Self::NotFound {
                entity: "download",
                id,
            },
            DownloadError::AlreadyQueued { id } => {
                Self::Conflict(format!("download already queued: {id}"))
            }
            DownloadError::Cancelled => Self::Conflict("download cancelled".to_string()),
            DownloadError::QueueFull { max_size } => {
                Self::Conflict(format!("queue full: max {max_size} downloads"))
            }
            _ => Self::Internal(err.to_string()),
        }
    }
}

/// What a core error is to a caller: a rejected setting or input is the
/// caller's to fix, a missing row is not found, a duplicate is a conflict,
/// and only a failure of the store itself is internal. The same table as
/// `gglib-axum`'s `From<CoreError> for HttpError`, so an error is the same
/// status whether or not it passed through here.
impl From<gglib_core::CoreError> for GuiError {
    fn from(err: gglib_core::CoreError) -> Self {
        use gglib_core::CoreError;
        match err {
            CoreError::Repository(repository) => repository.into(),
            CoreError::Settings(refused) => Self::ValidationFailed(refused.to_string()),
            CoreError::Validation(msg) => Self::ValidationFailed(msg),
        }
    }
}

impl From<gglib_core::ports::RepositoryError> for GuiError {
    fn from(err: gglib_core::ports::RepositoryError) -> Self {
        use gglib_core::ports::RepositoryError;
        match err {
            // A repository names what it missed in its own words, not by id.
            RepositoryError::NotFound(what) => Self::NotFound {
                entity: "record",
                id: what,
            },
            RepositoryError::AlreadyExists(msg) => Self::Conflict(msg),
            RepositoryError::Constraint(msg) => Self::ValidationFailed(msg),
            RepositoryError::Storage(_) | RepositoryError::Serialization(_) => {
                Self::Internal(err.to_string())
            }
        }
    }
}

impl GuiError {
    /// The install prompt for a llama-server that cannot be used, and why.
    ///
    /// Names the path gglib looks for llama-server at and the command that
    /// installs it. The path is empty when it cannot be resolved.
    pub(crate) fn llama_server_not_installed(reason: &str) -> Self {
        Self::LlamaServerNotInstalled {
            expected_path: llama_server_path()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            suggested_command: LLAMA_INSTALL_COMMAND.to_string(),
            reason: reason.to_string(),
        }
    }

    /// This error, its message led by what was being done when it happened.
    /// The kind is kept, so a refusal stays a refusal; a not-found already
    /// names what was missing and is left as it is.
    #[must_use]
    pub fn context(self, doing: &str) -> Self {
        let led = |msg: String| format!("{doing}: {msg}");
        match self {
            Self::ValidationFailed(msg) => Self::ValidationFailed(led(msg)),
            Self::Conflict(msg) => Self::Conflict(led(msg)),
            Self::Unavailable(msg) => Self::Unavailable(led(msg)),
            Self::Internal(msg) => Self::Internal(led(msg)),
            named @ (Self::NotFound { .. } | Self::LlamaServerNotInstalled { .. }) => named,
        }
    }
}

impl From<gglib_core::McpServiceError> for GuiError {
    fn from(err: gglib_core::McpServiceError) -> Self {
        use gglib_core::McpServiceError;
        match err {
            McpServiceError::Repository(e) => Self::Internal(e.to_string()),
            McpServiceError::NotRunning(name) => {
                Self::Conflict(format!("MCP server not running: {name}"))
            }
            _ => Self::Internal(err.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
