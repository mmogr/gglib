//! Tauri-specific error types and mappings.
//!
//! This module provides error types for the Tauri adapter and mappings
//! from CoreError to serializable error responses for the frontend.

use gglib_core::{CoreError, ProcessError, RepositoryError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Tauri-specific error type.
///
/// This error type is serializable for sending to the frontend via IPC.
/// The `#[serde(tag = "type", content = "message")]` attribute produces
/// JSON like `{"type": "NotFound", "message": "Model not found"}`.
#[derive(Debug, Error, Serialize, Deserialize)]
#[serde(tag = "type", content = "message")]
pub enum TauriError {
    /// Resource not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Invalid input from frontend.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Database/storage error.
    #[error("Database error: {0}")]
    Database(String),

    /// Process management error.
    #[error("Process error: {0}")]
    Process(String),

    /// External service error.
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    /// Internal error.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<CoreError> for TauriError {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::Repository(repo_err) => repo_err.into(),
            CoreError::Process(proc_err) => proc_err.into(),
            CoreError::Settings(settings_err) => TauriError::InvalidInput(settings_err.to_string()),
            CoreError::Validation(msg) => TauriError::InvalidInput(msg),
            CoreError::Configuration(msg) => TauriError::Internal(format!("Config: {}", msg)),
            CoreError::ExternalService(msg) => TauriError::ServiceUnavailable(msg),
            CoreError::Internal(msg) => TauriError::Internal(msg),
        }
    }
}

impl From<RepositoryError> for TauriError {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::NotFound(msg) => TauriError::NotFound(msg),
            RepositoryError::AlreadyExists(msg) => {
                TauriError::InvalidInput(format!("Already exists: {}", msg))
            }
            RepositoryError::Storage(msg) => TauriError::Database(msg),
            RepositoryError::Serialization(msg) => {
                TauriError::Internal(format!("Serialization: {}", msg))
            }
            RepositoryError::Constraint(msg) => TauriError::InvalidInput(msg),
        }
    }
}

impl From<ProcessError> for TauriError {
    fn from(err: ProcessError) -> Self {
        match err {
            ProcessError::NotRunning(msg) => TauriError::Process(format!("Not running: {}", msg)),
            ProcessError::StartFailed(msg) => TauriError::Process(format!("Start failed: {}", msg)),
            ProcessError::StopFailed(msg) => TauriError::Process(format!("Stop failed: {}", msg)),
            ProcessError::HealthCheckFailed(msg) => {
                TauriError::Process(format!("Health check: {}", msg))
            }
            ProcessError::Configuration(msg) => TauriError::InvalidInput(msg),
            ProcessError::ResourceExhausted(msg) => {
                TauriError::Process(format!("Resource exhausted: {}", msg))
            }
            ProcessError::Internal(msg) => TauriError::Internal(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_serialization() {
        let err = TauriError::NotFound("Model 42".to_string());
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("NotFound"));
        assert!(json.contains("Model 42"));
    }

    #[test]
    fn test_core_error_conversion() {
        let core_err = CoreError::Validation("Invalid name".to_string());
        let tauri_err: TauriError = core_err.into();
        assert!(matches!(tauri_err, TauriError::InvalidInput(_)));
    }
}
