//! Infrastructure layer implementations.
//!
//! This module contains concrete implementations of the port traits defined
//! in `core::ports`. All infrastructure concerns (database, filesystem,
//! process management) are confined here.
//!
//! # Structure
//!
//! - `repositories` - Database implementations of repository traits (now in gglib-db)
//! - `mappers` - Boundary conversions between legacy and domain types
//! - `process` - Process runner implementations (now in gglib-runtime)

pub mod mappers;
pub mod repositories;

// Re-export commonly used implementations
pub use repositories::{SqliteModelRepository, SqliteSettingsRepository};

// NOTE: LlamaProcessRunner is now in gglib-runtime crate as LlamaServerRunner
// The legacy process module is retained for now but not re-exported.
