//! Port definitions (trait abstractions) for external systems.
//!
//! **MIGRATION SHIM**: Repository and process runner ports are now in `gglib_core`.
//! This module retains only download/HF traits not yet migrated.
//!
//! # Design Rules
//!
//! - No `sqlx` types in any signature
//! - No process/filesystem implementation details
//! - Traits are minimal and CRUD-focused for repositories
//! - Intent-based methods for process runner (not implementation-leaking)

// SHIM: These submodules are deprecated, use gglib_core directly
// Keeping for backwards compatibility during migration
pub mod events {
    //! Events re-exported from gglib_core.
    pub use gglib_core::events::{AppEvent, ModelSummary, ServerSnapshotEntry};
}

pub mod model_repository {
    //! Model repository re-exported from gglib_core.
    pub use gglib_core::ports::model_repository::ModelRepository;
}

pub mod process_runner {
    //! Process runner re-exported from gglib_core.
    pub use gglib_core::ports::process_runner::{
        ProcessHandle, ProcessRunner, ServerConfig, ServerHealth,
    };
}

pub mod settings_repository {
    //! Settings repository re-exported from gglib_core.
    pub use gglib_core::ports::settings_repository::SettingsRepository;
}

// Re-export download/HF traits from their current locations (not yet migrated to crate)
pub use crate::download::domain::traits::{
    DownloadExecutor, EventCallback, ExecuteParams, ExecutionResult, QuantizationResolver,
    Resolution, ResolvedFile,
};
pub use crate::services::huggingface::HttpBackend;

// SHIM: Re-export error types from gglib_core (remove duplicates)
pub use gglib_core::{ProcessError, RepositoryError};

