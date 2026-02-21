//! Semantic error types for GUI operations.
//!
//! These errors are domain-focused, not HTTP-focused. Adapters map
//! `GuiError` to their specific error types (TauriError, HttpError).

use std::fmt;

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
        /// Optional legacy path where an old installation was detected
        legacy_path: Option<String>,
        /// Suggested command to fix the issue
        suggested_command: String,
        /// Reason for the failure (NotFound, NotExecutable, PermissionDenied)
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
                legacy_path,
                suggested_command,
                reason,
            } => {
                write!(
                    f,
                    "llama-server not installed: {} at {}\nRun: {}",
                    reason, expected_path, suggested_command
                )?;
                if let Some(legacy) = legacy_path {
                    write!(f, "\nFound older installation at: {}", legacy)?;
                }
                Ok(())
            }
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for GuiError {}

// ============================================================================
// Conversions from core errors
// ============================================================================

impl From<gglib_core::ports::VoicePortError> for GuiError {
    fn from(err: gglib_core::ports::VoicePortError) -> Self {
        use gglib_core::ports::VoicePortError;
        match err {
            VoicePortError::NotFound(msg) => Self::NotFound {
                entity: "voice",
                id: msg,
            },
            VoicePortError::AlreadyActive => {
                Self::Conflict("voice pipeline already active".to_string())
            }
            VoicePortError::NotActive => Self::Conflict(
                "voice pipeline not active — call /api/voice/start first".to_string(),
            ),
            VoicePortError::NotInitialised => {
                Self::Unavailable("voice pipeline not initialised".to_string())
            }
            VoicePortError::LoadError(msg) => Self::Internal(format!("voice load error: {msg}")),
            VoicePortError::DownloadError(msg) => {
                Self::Internal(format!("voice download error: {msg}"))
            }
            VoicePortError::Internal(msg) => Self::Internal(msg),
        // 400 Bad Request: caller should change the request (switch to PTT mode)
        // rather than retry. 503 Unavailable would imply a transient failure.
        VoicePortError::Unimplemented(msg) => Self::ValidationFailed(msg),
        }
    }
}

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

impl From<gglib_core::CoreError> for GuiError {
    fn from(err: gglib_core::CoreError) -> Self {
        Self::Internal(err.to_string())
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
