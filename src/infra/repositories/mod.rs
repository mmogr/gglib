//! Repository implementations using SQLite.
//!
//! **MIGRATION SHIM**: Repository implementations are now in `gglib_db` crate.
//! This module re-exports for backwards compatibility during migration.

// SHIM: Re-export from gglib_db crate (remove in Phase 5 final cleanup)
pub use gglib_db::{SqliteModelRepository, SqliteSettingsRepository};

