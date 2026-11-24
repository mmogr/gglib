//! Semantic error types for GUI operations.
//!
//! These errors are domain-focused, not HTTP-focused. Adapters map
//! `GuiError` to their specific error types (`TauriError`, `HttpError`).

use std::fmt;
use std::time::Duration;

use gglib_core::download::DownloadError;
use gglib_core::hub_limit::HubBucket;
use gglib_core::paths::{LLAMA_INSTALL_COMMAND, llama_server_path};
use gglib_core::ports::HfPortError;

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

    /// Hugging Face's rate limit refused a request the operation needed, or
    /// would have (429-ish). The same operation asked again once it lifts
    /// can go ahead.
    HubLimited {
        /// The allowance that is spent.
        bucket: HubBucket,
        /// The time until it opens again.
        retry_after: Duration,
        /// What was being done, and when the limit lifts.
        message: String,
    },

    /// A model did not load, and why: the runtime's
    /// [`LoadFailure`](gglib_core::ports::LoadFailure) in its own words,
    /// which every surface shows as they stand.
    LoadFailed(String),

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
            Self::HubLimited { message, .. } | Self::LoadFailed(message) => f.write_str(message),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for GuiError {}

// ============================================================================
// Conversions from core errors
// ============================================================================

impl From<DownloadError> for GuiError {
    fn from(err: DownloadError) -> Self {
        if let Some((bucket, retry_after)) = err.hub_limit() {
            return Self::HubLimited {
                bucket,
                retry_after,
                message: err.to_string(),
            };
        }
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
            Self::HubLimited {
                bucket,
                retry_after,
                message,
            } => Self::HubLimited {
                bucket,
                retry_after,
                message: led(message),
            },
            // A failed load is its own sentence, which already says what.
            named @ (Self::NotFound { .. }
            | Self::LlamaServerNotInstalled { .. }
            | Self::LoadFailed(_)) => named,
        }
    }
}

/// The Hub's errors as an operation that asked it answers them: its rate
/// limit is [`GuiError::HubLimited`], which a caller can wait out and ask
/// again, and any other is what it has always been.
impl GuiError {
    /// `error`, which the Hub answered while `doing`, led by `doing`: its
    /// rate limit is [`Self::HubLimited`], and any other error is internal.
    pub(crate) fn from_hub(doing: &str, error: HfPortError) -> Self {
        Self::from_hub_or(doing, error, |e| Self::Internal(format!("{doing}: {e}")))
    }

    /// [`Self::from_hub`], with any error that is not the rate limit made
    /// by `otherwise`.
    pub(crate) fn from_hub_or(
        doing: &str,
        error: HfPortError,
        otherwise: impl FnOnce(HfPortError) -> Self,
    ) -> Self {
        match error {
            HfPortError::RateLimited { bucket, resets_in } => Self::HubLimited {
                bucket,
                retry_after: resets_in,
                message: format!("{doing}: {error}"),
            },
            other => otherwise(other),
        }
    }

    /// `error`, a download's, met while `doing`: the Hub's rate limit is
    /// [`Self::HubLimited`], led by `doing`, and any other error is what
    /// `otherwise` makes of it.
    pub(crate) fn from_download_or(
        doing: &str,
        error: DownloadError,
        otherwise: impl FnOnce(DownloadError) -> Self,
    ) -> Self {
        if error.hub_limit().is_some() {
            Self::from(error).context(doing)
        } else {
            otherwise(error)
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
            // The caller's to fix, by choosing another name.
            McpServiceError::NameTaken(_) => Self::Conflict(err.to_string()),
            // The caller's too: the request gives a server the name gglib
            // keeps, opens that name to paired devices, or stores a blank
            // value.
            McpServiceError::NameReserved(_)
            | McpServiceError::NeverOpened(_)
            | McpServiceError::BlankEnvValue(_) => Self::ValidationFailed(err.to_string()),
            _ => Self::Internal(err.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
