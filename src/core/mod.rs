//! Core domain types and port definitions.
//!
//! **MIGRATION SHIM**: This module re-exports from `gglib_core` crate.
//! Direct usage of submodules is deprecated — import from the crate or this module.
//!
//! # Structure
//!
//! - `domain` - Core domain types (`Model`, `NewModel`, server configuration)
//! - `ports` - Trait definitions for repositories and external systems

// SHIM: Re-export domain module from crate (remove in Phase 5 final cleanup)
pub mod domain {
    //! Domain types re-exported from gglib_core.
    pub use gglib_core::domain::{Model, NewModel};
}

// Keep local ports module for download/HF traits not yet migrated
pub mod ports;

// Re-export commonly used types for convenience
// SHIM: These come from gglib_core now
pub use gglib_core::{
    AppEvent, Model, ModelRepository, NewModel, ProcessError, ProcessHandle, ProcessRunner,
    RepositoryError, ServerConfig, ServerHealth, SettingsRepository,
};

// Re-export download traits from their current locations (not yet migrated to crate)
pub use ports::{
    DownloadExecutor, EventCallback, ExecuteParams, ExecutionResult,
    QuantizationResolver, Resolution, ResolvedFile,
};
