//! Download error types.
//!
//! These errors are designed to be serializable and not depend on external
//! error types like `std::io::Error`. For I/O errors, we capture the kind
//! and message as strings.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Error type for download operations.
///
/// Designed to be serializable across FFI boundaries (Tauri, CLI, etc.)
/// without depending on non-serializable types like `std::io::Error`.
#[derive(Clone, Debug, Error, Serialize, Deserialize, PartialEq, Eq)]
pub enum DownloadError {
    /// I/O error during file operations.
    #[error("I/O error ({kind}): {message}")]
    Io {
        /// The kind of I/O error (e.g., "not found", "permission denied").
        kind: String,
        /// Detailed error message.
        message: String,
    },

    /// Network/HTTP error during download.
    #[error("Network error: {message}")]
    Network {
        /// Detailed error message.
        message: String,
        /// HTTP status code if available.
        #[serde(skip_serializing_if = "Option::is_none")]
        status_code: Option<u16>,
    },

    /// Model or file not found on the remote server.
    #[error("Not found: {message}")]
    NotFound {
        /// What was not found (model ID, file, etc.).
        message: String,
    },

    /// Invalid quantization specified.
    #[error("Invalid quantization: {value}")]
    InvalidQuantization {
        /// The invalid quantization string.
        value: String,
    },

    /// Failed to resolve quantization to a file.
    #[error("Resolution failed: {message}")]
    ResolutionFailed {
        /// Detailed error message.
        message: String,
    },

    /// Queue is full, cannot add more downloads.
    #[error("Queue full: maximum {max_size} downloads allowed")]
    QueueFull {
        /// Maximum queue capacity.
        max_size: u32,
    },

    /// Download is already queued.
    #[error("Already queued: {id}")]
    AlreadyQueued {
        /// The download ID that's already in the queue.
        id: String,
    },

    /// Download not found in queue.
    #[error("Not in queue: {id}")]
    NotInQueue {
        /// The download ID that wasn't found.
        id: String,
    },

    /// Download was cancelled by user.
    #[error("Download cancelled")]
    Cancelled,

    /// Download was interrupted and can be resumed.
    #[error("Download interrupted at {bytes_downloaded} bytes")]
    Interrupted {
        /// Bytes downloaded before interruption.
        bytes_downloaded: u64,
    },

    /// Integrity check failed (checksum mismatch).
    #[error("Integrity check failed: expected {expected}, got {actual}")]
    IntegrityFailed {
        /// Expected checksum.
        expected: String,
        /// Actual checksum computed.
        actual: String,
    },

    /// General/uncategorized error.
    #[error("{message}")]
    Other {
        /// Error message.
        message: String,
    },
}

impl DownloadError {
    /// Create an I/O error from kind and message strings.
    pub fn io(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Io {
            kind: kind.into(),
            message: message.into(),
        }
    }

    /// Create an I/O error from a `std::io::Error`.
    ///
    /// This captures the error kind name and message for serialization.
    #[must_use]
    pub fn from_io_error(err: &std::io::Error) -> Self {
        let kind = err.kind();
        Self::Io {
            kind: format!("{kind:?}"),
            message: err.to_string(),
        }
    }

    /// Create a network error.
    pub fn network(message: impl Into<String>) -> Self {
        Self::Network {
            message: message.into(),
            status_code: None,
        }
    }

    /// Create a network error with HTTP status code.
    pub fn network_with_status(message: impl Into<String>, status_code: u16) -> Self {
        Self::Network {
            message: message.into(),
            status_code: Some(status_code),
        }
    }

    /// Create a not found error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    /// Create an invalid quantization error.
    pub fn invalid_quantization(value: impl Into<String>) -> Self {
        Self::InvalidQuantization {
            value: value.into(),
        }
    }

    /// Create a resolution failed error.
    pub fn resolution_failed(message: impl Into<String>) -> Self {
        Self::ResolutionFailed {
            message: message.into(),
        }
    }

    /// Create a queue full error.
    #[must_use]
    pub const fn queue_full(max_size: u32) -> Self {
        Self::QueueFull { max_size }
    }

    /// Create an already queued error.
    pub fn already_queued(id: impl Into<String>) -> Self {
        Self::AlreadyQueued { id: id.into() }
    }

    /// Create a not in queue error.
    pub fn not_in_queue(id: impl Into<String>) -> Self {
        Self::NotInQueue { id: id.into() }
    }

    /// Create an integrity check failed error.
    pub fn integrity_failed(expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self::IntegrityFailed {
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    /// Create a generic error.
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other {
            message: message.into(),
        }
    }

    /// Check if this error is recoverable (can retry).
    #[must_use]
    pub const fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::Network { .. } | Self::Interrupted { .. } | Self::Io { .. }
        )
    }

    /// Check if this is a cancellation.
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    /// Convert to a user-friendly message.
    #[must_use]
    pub fn user_message(&self) -> String {
        match self {
            Self::Io { message, .. } => format!("File operation failed: {message}"),
            Self::Network {
                message,
                status_code: Some(code),
            } => {
                format!("Network error (HTTP {code}): {message}")
            }
            Self::Network { message, .. } => format!("Network error: {message}"),
            Self::NotFound { message } => format!("Not found: {message}"),
            Self::InvalidQuantization { value } => {
                format!("Invalid quantization '{value}'. Use values like `Q4_K_M`, `Q5_K_S`, etc.")
            }
            Self::ResolutionFailed { message } => format!("Could not resolve file: {message}"),
            Self::QueueFull { max_size } => {
                format!(
                    "Download queue is full (max {max_size} items). Wait for a download to complete."
                )
            }
            Self::AlreadyQueued { id } => {
                format!("Download '{id}' is already in the queue.")
            }
            Self::NotInQueue { id } => {
                format!("Download '{id}' is not in the queue.")
            }
            Self::Cancelled => "Download was cancelled.".to_string(),
            Self::Interrupted { bytes_downloaded } => {
                format!("Download interrupted after {bytes_downloaded} bytes. You can resume it.")
            }
            Self::IntegrityFailed { .. } => {
                "File integrity check failed. The download may be corrupted.".to_string()
            }
            Self::Other { message } => message.clone(),
        }
    }
}

/// Convenience result type for download operations.
pub type DownloadResult<T> = Result<T, DownloadError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_error_from_std() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = DownloadError::from_io_error(&io_err);

        match err {
            DownloadError::Io { kind, message } => {
                assert_eq!(kind, "NotFound");
                assert!(message.contains("file not found"));
            }
            _ => panic!("Expected Io variant"),
        }
    }

    #[test]
    fn test_error_serialization() {
        let err = DownloadError::network_with_status("timeout", 408);
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("408"));
        assert!(json.contains("timeout"));

        let parsed: DownloadError = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, err);
    }

    #[test]
    fn test_is_recoverable() {
        assert!(DownloadError::network("timeout").is_recoverable());
        assert!(
            DownloadError::Interrupted {
                bytes_downloaded: 100
            }
            .is_recoverable()
        );
        assert!(!DownloadError::Cancelled.is_recoverable());
        assert!(!DownloadError::invalid_quantization("bad").is_recoverable());
    }

    #[test]
    fn test_user_messages() {
        let err = DownloadError::queue_full(5);
        assert!(err.user_message().contains('5'));
        assert!(err.user_message().contains("full"));
    }
}
