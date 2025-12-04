//! GGUF parsing error types.
//!
//! This module provides typed error handling for GGUF file parsing operations.

use std::fmt;
use std::io;

/// Errors that can occur when parsing GGUF files.
#[derive(Debug)]
pub enum GgufError {
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
}

impl fmt::Display for GgufError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GgufError::InvalidMagic => write!(f, "Invalid GGUF file: wrong magic number"),
            GgufError::UnsupportedVersion(v) => write!(f, "Unsupported GGUF version: {}", v),
            GgufError::Io(e) => write!(f, "I/O error: {}", e),
            GgufError::Utf8Error => write!(f, "Invalid UTF-8 string in GGUF file"),
            GgufError::InvalidValueType(t) => write!(f, "Unknown GGUF value type: {}", t),
        }
    }
}

impl std::error::Error for GgufError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GgufError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for GgufError {
    fn from(err: io::Error) -> Self {
        GgufError::Io(err)
    }
}

/// A specialized Result type for GGUF operations.
pub type GgufResult<T> = Result<T, GgufError>;
