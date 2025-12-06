//! Port definitions (trait abstractions) for external systems.
//!
//! Ports define the interfaces that the core domain expects from infrastructure.
//! They contain no implementation details and use only domain types.
//!
//! # Design Rules
//!
//! - No `sqlx` types in any signature
//! - No process/filesystem implementation details
//! - Traits are minimal and CRUD-focused for repositories
//! - Intent-based methods for process runner (not implementation-leaking)

pub mod model_repository;
pub mod process_runner;
pub mod settings_repository;

use thiserror::Error;

// Re-export repository traits for convenience
pub use model_repository::ModelRepository;
pub use process_runner::{ProcessHandle, ProcessRunner, ServerConfig, ServerHealth};
pub use settings_repository::SettingsRepository;

/// Domain-specific errors for repository operations.
///
/// This error type abstracts away storage implementation details (e.g., sqlx errors)
/// and provides a clean interface for services to handle storage failures.
#[derive(Debug, Error)]
pub enum RepositoryError {
    /// The requested entity was not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// An entity with the same identifier already exists.
    #[error("Already exists: {0}")]
    AlreadyExists(String),

    /// Storage backend error (database, filesystem, etc.).
    #[error("Storage error: {0}")]
    Storage(String),

    /// Serialization or deserialization failed.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// A constraint was violated (e.g., foreign key, unique constraint).
    #[error("Constraint violation: {0}")]
    Constraint(String),
}

/// Domain-specific errors for process runner operations.
///
/// This error type abstracts away process management implementation details
/// and provides a clean interface for services to handle process failures.
#[derive(Debug, Error)]
pub enum ProcessError {
    /// Failed to start the process.
    #[error("Failed to start: {0}")]
    StartFailed(String),

    /// Failed to stop the process.
    #[error("Failed to stop: {0}")]
    StopFailed(String),

    /// The process is not running.
    #[error("Process not running: {0}")]
    NotRunning(String),

    /// Health check failed.
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Resource exhaustion (e.g., no available ports).
    #[error("Resource exhaustion: {0}")]
    ResourceExhausted(String),

    /// Internal process error.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Core error type for semantic domain errors.
///
/// This is the canonical error type used across the core domain.
/// Adapters should map this to their own error types (HTTP status codes,
/// CLI exit codes, Tauri serialized errors).
#[derive(Debug, Error)]
pub enum CoreError {
    /// Repository operation failed.
    #[error(transparent)]
    Repository(#[from] RepositoryError),

    /// Process operation failed.
    #[error(transparent)]
    Process(#[from] ProcessError),

    /// Validation error (invalid input).
    #[error("Validation error: {0}")]
    Validation(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// External service error.
    #[error("External service error: {0}")]
    ExternalService(String),

    /// Internal error (unexpected condition).
    #[error("Internal error: {0}")]
    Internal(String),
}
