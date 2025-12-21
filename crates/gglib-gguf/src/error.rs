//! Internal error types for GGUF parsing.
//!
//! These errors provide detailed information about parsing failures.
//! They convert to `GgufParseError` from `gglib-core` for the port API.

use std::io;

use gglib_core::GgufParseError;

/// Internal errors that can occur during GGUF parsing.
///
/// More detailed than `GgufParseError`, used internally for debugging.
/// Converts to `GgufParseError` via `From` implementation.
#[derive(Debug)]
pub enum GgufInternalError {
    /// The file does not exist.
    FileNotFound(String),

    /// The file does not have a valid GGUF magic number.
    InvalidMagic,

    /// The GGUF version is not supported (only versions 1-3 are supported).
    UnsupportedVersion(u32),

    /// An I/O error occurred while reading the file.
    Io(io::Error),

    /// Invalid UTF-8 was encountered in a string field.
    Utf8Error,

    /// An unknown value type was encountered.
    InvalidValueType(u32),

    /// Memory mapping failed.
    #[cfg(feature = "mmap")]
    MmapError(String),
}

impl std::fmt::Display for GgufInternalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileNotFound(path) => write!(f, "File not found: {path}"),
            Self::InvalidMagic => write!(f, "Invalid GGUF file: wrong magic number"),
            Self::UnsupportedVersion(v) => write!(f, "Unsupported GGUF version: {v}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Utf8Error => write!(f, "Invalid UTF-8 string in GGUF file"),
            Self::InvalidValueType(t) => write!(f, "Unknown GGUF value type: {t}"),
            #[cfg(feature = "mmap")]
            Self::MmapError(msg) => write!(f, "Memory mapping error: {msg}"),
        }
    }
}

impl std::error::Error for GgufInternalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for GgufInternalError {
    fn from(err: io::Error) -> Self {
        if err.kind() == io::ErrorKind::NotFound {
            Self::FileNotFound(err.to_string())
        } else {
            Self::Io(err)
        }
    }
}

/// Convert internal errors to the domain-facing port error.
impl From<GgufInternalError> for GgufParseError {
    fn from(err: GgufInternalError) -> Self {
        match err {
            GgufInternalError::FileNotFound(path) => Self::NotFound(path),
            GgufInternalError::InvalidMagic => {
                Self::InvalidFormat("Invalid magic number".to_string())
            }
            GgufInternalError::UnsupportedVersion(v) => {
                Self::InvalidFormat(format!("Unsupported version: {v}"))
            }
            GgufInternalError::Io(e) => Self::Io(e.to_string()),
            GgufInternalError::Utf8Error => Self::InvalidFormat("Invalid UTF-8 string".to_string()),
            GgufInternalError::InvalidValueType(t) => {
                Self::InvalidFormat(format!("Unknown value type: {t}"))
            }
            #[cfg(feature = "mmap")]
            GgufInternalError::MmapError(msg) => Self::Io(msg),
        }
    }
}

/// Result type for internal GGUF operations.
pub type GgufResult<T> = Result<T, GgufInternalError>;
