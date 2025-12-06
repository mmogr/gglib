//! Tauri-specific error types and mappings.
//!
//! This module provides error types for the Tauri adapter and mappings
//! from CoreError to serializable error responses for the frontend.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Tauri-specific error type.
///
/// This error type is serializable for sending to the frontend via IPC.
#[derive(Debug, Error, Serialize, Deserialize)]
#[serde(tag = "type", content = "message")]
pub enum TauriError {
    /// Core domain error.
    #[error("{0}")]
    Core(String),

    /// Resource not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Invalid input from frontend.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Internal error.
    #[error("Internal error: {0}")]
    Internal(String),
}

// Placeholder: impl From<CoreError> for TauriError will be added during extraction
