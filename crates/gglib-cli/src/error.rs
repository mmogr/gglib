//! CLI-specific error types and mappings.
//!
//! This module provides error types for the CLI adapter and mappings
//! from CoreError to exit codes and user-facing messages.

use gglib_core::CoreError;
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

    /// Database error.
    #[error("Database error: {0}")]
    Database(String),

    /// Process execution error.
    #[error("Process error: {0}")]
    Process(String),
}

impl CliError {}

impl From<CoreError> for CliError {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::Repository(repo_err) => CliError::Database(repo_err.to_string()),
            CoreError::Process(proc_err) => CliError::Process(proc_err.to_string()),
            CoreError::Settings(settings_err) => CliError::Config(settings_err.to_string()),
            CoreError::Validation(msg) => CliError::Arguments(msg),
            CoreError::Configuration(msg) => CliError::Config(msg),
            CoreError::ExternalService(msg) => CliError::Core(format!("External service: {}", msg)),
            CoreError::Internal(msg) => CliError::Core(msg),
        }
    }
}

impl From<std::io::Error> for CliError {
    fn from(err: std::io::Error) -> Self {
        CliError::Io(err.to_string())
    }
}
