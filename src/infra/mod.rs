//! Infrastructure layer implementations.
//!
//! This module contains concrete implementations of the port traits defined
//! in `core::ports`. All infrastructure concerns (database, filesystem,
//! process management) are confined here.
//!
//! # Structure
//!
//! - `repositories` - Database implementations of repository traits
//! - `mappers` - Boundary conversions between legacy and domain types
//! - `process` - Process runner implementations

pub mod mappers;
pub mod process;
pub mod repositories;

// Re-export commonly used implementations
pub use process::LlamaProcessRunner;
pub use repositories::{SqliteModelRepository, SqliteSettingsRepository};
