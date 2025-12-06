//! Core domain types and port definitions.
//!
//! This module contains the pure domain logic, traits (ports), and domain types
//! that form the heart of the application. No infrastructure dependencies (sqlx,
//! filesystem, process management) should appear here.
//!
//! # Structure
//!
//! - `domain` - Core domain types (`Model`, `NewModel`, server configuration)
//! - `ports` - Trait definitions for repositories and external systems

pub mod domain;
pub mod ports;

// Re-export commonly used types for convenience
pub use domain::{Model, NewModel};
pub use ports::{
    DownloadExecutor,
    EventCallback,
    ExecuteParams,
    ExecutionResult,
    // Re-export existing traits (canonical import path)
    HttpBackend,
    ProcessError,
    QuantizationResolver,
    RepositoryError,
    Resolution,
    ResolvedFile,
    events::AppEvent,
    model_repository::ModelRepository,
    process_runner::{ProcessHandle, ProcessRunner, ServerConfig, ServerHealth},
    settings_repository::SettingsRepository,
};
