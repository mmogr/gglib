//! CLI-specific error types and mappings.
//!
//! This module provides error types for the CLI adapter and mappings
//! from CoreError to exit codes and user-facing messages.

use thiserror::Error;

/// CLI-specific error type.
#[derive(Debug, Error)]
pub enum CliError {
    /// Core domain error.
    #[error("{0}")]
    Core(String),

    /// Argument parsing error.
    #[error("Invalid arguments: {0}")]
    Arguments(String),

    /// IO error (file not found, permission denied, etc.).
    #[error("IO error: {0}")]
    Io(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),
}

impl CliError {
    /// Map error to appropriate exit code.
    ///
    /// Exit codes follow Unix conventions:
    /// - 0: Success
    /// - 1: General error
    /// - 2: Misuse of shell command (invalid arguments)
    /// - 64-78: Reserved for specific error categories (see sysexits.h)
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Core(_) => 1,
            CliError::Arguments(_) => 2,
            CliError::Io(_) => 74,     // EX_IOERR
            CliError::Config(_) => 78, // EX_CONFIG
        }
    }
}

// Placeholder: impl From<CoreError> for CliError will be added during extraction
