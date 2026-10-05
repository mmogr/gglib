//! Protocol parsing for Python helper communication.
//!
//! Defines the JSON protocol between `hf_xet_downloader.py` and `python_bridge.rs`.
//! Each JSON line maps 1:1 to a `PythonEvent` variant.
//!
//! # Protocol Schema
//!
//! All messages are JSON objects with a required `status` field:
//!
//! ```json
//! {"status": "progress", "written": 123456, "received": 150000, "total": 789012}
//! {"status": "unavailable", "reason": "xet not supported for this repo"}
//! {"status": "error", "message": "Network timeout"}
//! {"status": "complete"}
//! ```
//!
//! A progress line is about the one file the helper was started for.
//! `written` is bytes of it on disk, `received` is bytes off the network in
//! this run, and a `total` of 0 means the size is not known.

use serde::Deserialize;
use thiserror::Error;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur when parsing protocol messages.
#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error("Missing or invalid 'status' field")]
    InvalidStatus,

    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    #[error("Unknown status: {0}")]
    UnknownStatus(String),
}

// ============================================================================
// Protocol Events
// ============================================================================

/// Events emitted by the Python helper script.
///
/// Maps 1:1 to the JSON protocol schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PythonEvent {
    /// Download progress update.
    Progress {
        /// Bytes of the file on disk.
        written: u64,
        /// Bytes received from the network in this run.
        received: u64,
        /// The file's size, 0 when not known.
        total: u64,
    },

    /// Fast download is unavailable for this repository.
    Unavailable {
        /// Human-readable reason why fast download is unavailable.
        reason: String,
    },

    /// An error occurred during download.
    Error {
        /// Human-readable error message.
        message: String,
    },

    /// Download completed successfully.
    Complete,
}

// ============================================================================
// Parsing
// ============================================================================

/// Raw JSON envelope for parsing.
#[derive(Deserialize)]
struct RawEnvelope {
    status: Option<String>,
    // Progress fields
    written: Option<u64>,
    received: Option<u64>,
    total: Option<u64>,
    // Error/unavailable fields
    message: Option<String>,
    reason: Option<String>,
    detail: Option<String>,
}

/// Parse a single line of protocol output into a `PythonEvent`.
///
/// # Arguments
///
/// * `line` - A single JSON line from the Python helper's stdout.
///
/// # Returns
///
/// - `Ok(PythonEvent)` if the line is valid protocol JSON.
/// - `Err(ProtocolError)` if the line is malformed or missing required fields.
///
/// # Examples
///
/// ```ignore
/// let event = parse_line(r#"{"status": "progress", "written": 100, "received": 100, "total": 200}"#)?;
/// assert!(matches!(event, PythonEvent::Progress { .. }));
/// ```
pub(crate) fn parse_line(line: &str) -> Result<PythonEvent, ProtocolError> {
    let envelope: RawEnvelope = serde_json::from_str(line)?;

    let status = envelope.status.ok_or(ProtocolError::InvalidStatus)?;

    match status.as_str() {
        "progress" => {
            let written = envelope
                .written
                .ok_or(ProtocolError::MissingField("written"))?;
            let received = envelope
                .received
                .ok_or(ProtocolError::MissingField("received"))?;
            let total = envelope.total.ok_or(ProtocolError::MissingField("total"))?;

            Ok(PythonEvent::Progress {
                written,
                received,
                total,
            })
        }

        "unavailable" => {
            // Accept reason, detail, or message as the explanation
            let reason = envelope
                .reason
                .or(envelope.detail)
                .or(envelope.message)
                .ok_or(ProtocolError::MissingField("reason"))?;

            Ok(PythonEvent::Unavailable { reason })
        }

        "error" | "file-error" => {
            let message = envelope
                .message
                .or(envelope.detail)
                .ok_or(ProtocolError::MissingField("message"))?;

            Ok(PythonEvent::Error { message })
        }

        "complete" => Ok(PythonEvent::Complete),

        other => Err(ProtocolError::UnknownStatus(other.to_string())),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // Progress events
    // ------------------------------------------------------------------------

    #[test]
    fn test_parse_progress() {
        let line = r#"{"status": "progress", "written": 1000, "received": 1500, "total": 5000}"#;
        let event = parse_line(line).unwrap();

        assert_eq!(
            event,
            PythonEvent::Progress {
                written: 1000,
                received: 1500,
                total: 5000,
            }
        );
    }

    /// Each count is required: a line with one of them missing is from a
    /// helper that counts some other way, and guessing the other would put a
    /// wrong number on screen.
    #[test]
    fn progress_needs_written_and_received() {
        let no_received = r#"{"status": "progress", "written": 500, "total": 1000}"#;
        let err = parse_line(no_received).unwrap_err();
        assert!(matches!(err, ProtocolError::MissingField("received")));

        let no_written = r#"{"status": "progress", "received": 500, "total": 1000}"#;
        let err = parse_line(no_written).unwrap_err();
        assert!(matches!(err, ProtocolError::MissingField("written")));
    }

    #[test]
    fn test_parse_progress_missing_total() {
        let line = r#"{"status": "progress", "written": 500, "received": 500}"#;
        let err = parse_line(line).unwrap_err();

        assert!(matches!(err, ProtocolError::MissingField("total")));
    }

    // ------------------------------------------------------------------------
    // Unavailable events
    // ------------------------------------------------------------------------

    #[test]
    fn test_parse_unavailable_with_reason() {
        let line = r#"{"status": "unavailable", "reason": "xet not supported"}"#;
        let event = parse_line(line).unwrap();

        assert_eq!(
            event,
            PythonEvent::Unavailable {
                reason: "xet not supported".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_unavailable_with_detail() {
        let line = r#"{"status": "unavailable", "detail": "fallback reason"}"#;
        let event = parse_line(line).unwrap();

        assert_eq!(
            event,
            PythonEvent::Unavailable {
                reason: "fallback reason".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_unavailable_missing_reason() {
        let line = r#"{"status": "unavailable"}"#;
        let err = parse_line(line).unwrap_err();

        assert!(matches!(err, ProtocolError::MissingField("reason")));
    }

    // ------------------------------------------------------------------------
    // Error events
    // ------------------------------------------------------------------------

    #[test]
    fn test_parse_error_with_message() {
        let line = r#"{"status": "error", "message": "Network timeout"}"#;
        let event = parse_line(line).unwrap();

        assert_eq!(
            event,
            PythonEvent::Error {
                message: "Network timeout".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_file_error() {
        let line = r#"{"status": "file-error", "message": "File not found"}"#;
        let event = parse_line(line).unwrap();

        assert_eq!(
            event,
            PythonEvent::Error {
                message: "File not found".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_error_missing_message() {
        let line = r#"{"status": "error"}"#;
        let err = parse_line(line).unwrap_err();

        assert!(matches!(err, ProtocolError::MissingField("message")));
    }

    // ------------------------------------------------------------------------
    // Complete events
    // ------------------------------------------------------------------------

    #[test]
    fn test_parse_complete() {
        let line = r#"{"status": "complete"}"#;
        let event = parse_line(line).unwrap();

        assert_eq!(event, PythonEvent::Complete);
    }

    // ------------------------------------------------------------------------
    // Error cases
    // ------------------------------------------------------------------------

    #[test]
    fn test_parse_invalid_json() {
        let line = "not json at all";
        let err = parse_line(line).unwrap_err();

        assert!(matches!(err, ProtocolError::InvalidJson(_)));
    }

    #[test]
    fn test_parse_missing_status() {
        let line = r#"{"written": 100, "total": 200}"#;
        let err = parse_line(line).unwrap_err();

        assert!(matches!(err, ProtocolError::InvalidStatus));
    }

    #[test]
    fn test_parse_unknown_status() {
        let line = r#"{"status": "unknown_event_type"}"#;
        let err = parse_line(line).unwrap_err();

        assert!(matches!(err, ProtocolError::UnknownStatus(_)));
    }

    #[test]
    fn test_parse_empty_object() {
        let line = r"{}";
        let err = parse_line(line).unwrap_err();

        assert!(matches!(err, ProtocolError::InvalidStatus));
    }

    // ------------------------------------------------------------------------
    // Error display
    // ------------------------------------------------------------------------

    #[test]
    fn test_protocol_error_display() {
        let err = ProtocolError::MissingField("written");
        assert!(err.to_string().contains("written"));

        let err = ProtocolError::UnknownStatus("foo".to_string());
        assert!(err.to_string().contains("foo"));
    }
}
