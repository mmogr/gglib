//! Download error types.
//!
//! These errors are designed to be serializable and not depend on external
//! error types like `std::io::Error`. For I/O errors, we capture the kind
//! and message as strings.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::format::{format_duration, format_size};

/// What a download that gave up on a quiet connection says: what happened,
/// and that trying again carries on from the part already there.
/// With nothing downloaded, there is no part to carry on from, and it says so.
fn interrupted_sentence(bytes_downloaded: u64, quiet_secs: u32) -> String {
    let quiet = format_duration(Some(f64::from(quiet_secs)));
    if bytes_downloaded == 0 {
        return format!(
            "no data arrived for {quiet}, so the download stopped before any of the file arrived"
        );
    }
    format!(
        "no data arrived for {quiet}, so the download stopped; trying again carries on from the {} \
         already downloaded",
        format_size(bytes_downloaded)
    )
}

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

    /// The connection sent nothing for `quiet_secs` over every reconnect,
    /// so the download stopped. The bytes it has are kept, and trying again
    /// carries on from them.
    #[error("{}", interrupted_sentence(*bytes_downloaded, *quiet_secs))]
    Interrupted {
        /// Bytes of the file on disk, kept for the next try.
        bytes_downloaded: u64,
        /// How long nothing arrived before the download stopped, in seconds.
        #[serde(default)]
        quiet_secs: u32,
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

    /// A download that stopped after nothing arrived for `quiet_secs`, with
    /// `bytes_downloaded` of the file kept on disk.
    #[must_use]
    pub const fn interrupted(bytes_downloaded: u64, quiet_secs: u32) -> Self {
        Self::Interrupted {
            bytes_downloaded,
            quiet_secs,
        }
    }

    /// Create a generic error.
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other {
            message: message.into(),
        }
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
            Self::Interrupted { .. } => self.to_string(),
            Self::IntegrityFailed { .. } => {
                "File integrity check failed. The download may be corrupted.".to_string()
            }
            Self::Other { message } => message.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_serialization() {
        let err = DownloadError::network_with_status("timeout", 408);
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("408"));
        assert!(json.contains("timeout"));

        let parsed: DownloadError = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, err);
    }

    /// The sentence a download that gave up on a quiet connection ends with.
    #[test]
    fn an_interrupted_download_says_what_happened_and_that_a_retry_carries_on() {
        let err = DownloadError::interrupted(4_133_906_022, 120);
        assert_eq!(
            err.to_string(),
            "no data arrived for 2m 00s, so the download stopped; trying again carries on \
             from the 3.85 GiB already downloaded"
        );
        assert_eq!(err.user_message(), err.to_string());
    }

    /// With nothing downloaded there is no part to carry on from.
    #[test]
    fn an_interrupted_download_with_nothing_downloaded_says_so() {
        assert_eq!(
            DownloadError::interrupted(0, 120).to_string(),
            "no data arrived for 2m 00s, so the download stopped before any of the file arrived"
        );
    }

    #[test]
    fn test_user_messages() {
        let err = DownloadError::queue_full(5);
        assert!(err.user_message().contains('5'));
        assert!(err.user_message().contains("full"));
    }
}
