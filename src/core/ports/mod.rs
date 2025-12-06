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
//!
//! # Existing Traits
//!
//! The following traits already exist in the codebase and follow the port pattern.
//! They are re-exported here as the canonical import path for new code:
//!
//! - `HttpBackend` - HTTP client abstraction for HuggingFace API
//! - `QuantizationResolver` - Resolves quantization files from repos
//! - `DownloadExecutor` - Executes file downloads

pub mod events;
pub mod model_repository;
pub mod process_runner;
pub mod settings_repository;

use thiserror::Error;

// Re-export existing traits from their current locations
// These already follow the port pattern and will be physically moved in Phase 5
pub use crate::download::domain::traits::{
    DownloadExecutor, EventCallback, ExecuteParams, ExecutionResult, QuantizationResolver,
    Resolution, ResolvedFile,
};
pub use crate::services::huggingface::HttpBackend;

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
