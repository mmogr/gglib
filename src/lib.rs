//! # gglib-lib (Transitional Facade)
//!
//! **⚠️ This crate is transitional scaffolding and will be removed.**
//!
//! **New code MUST NOT use this crate.** Import workspace crates directly:
//!
//! - `gglib_core` - Domain types, ports, and traits
//! - `gglib_db` - Database implementations  
//! - `gglib_runtime` - Process management and llama.cpp integration
//! - `gglib_download` - Model downloading from HuggingFace
//! - `gglib_gguf` - GGUF file parsing
//!
//! ## Purpose
//!
//! This crate exists only as a convenience re-export layer during the migration
//! from a monolithic structure to workspace crates. Once all references are
//! eliminated, this crate will be deleted.
//!
//! ## Migration Status
//!
//! - ✅ Legacy modules removed
//! - ✅ All new code uses workspace crates directly
//! - 🔄 Integration tests still use this facade (will be moved)
//! - ⏳ Pending deletion when usage reaches zero

// =============================================================================
// Workspace Crate Re-exports
// =============================================================================

// Re-export download types from gglib-download
pub use gglib_download::{
    DownloadError, DownloadEvent, DownloadId, DownloadManagerConfig, DownloadManagerImpl,
    DownloadRequest, DownloadStatus, DownloadSummary, Quantization, QueueAutoResult, QueueSnapshot,
    ShardGroupId,
};

// Re-export GGUF domain types and port from gglib-core
pub use gglib_core::domain::gguf::GgufValue;
pub use gglib_core::{GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort};

// Re-export core domain types and ports
pub use gglib_core::{
    AppEvent, Model, ModelRepository, NewModel, ProcessError, ProcessHandle, ProcessRunner,
    RepositoryError, ServerConfig, ServerHealth, SettingsRepository,
};

// Re-export database implementations
pub use gglib_db::{SqliteModelRepository, SqliteSettingsRepository};

// Re-export runtime crate for process management
pub use gglib_runtime::LlamaServerRunner;

/// Re-export of gglib-core types for convenience.
pub mod core_types {
    pub use gglib_core::*;
}

/// Re-export of gglib-db types for convenience.
pub mod db {
    pub use gglib_db::*;
}
