//! Repository implementations using SQLite.
//!
//! These implementations encapsulate all SQL queries and database access.
//! The `SqlitePool` is confined to this module and never exposed through
//! the port trait signatures.

mod row_mappers;
mod sqlite_model_repository;
mod sqlite_settings_repository;

pub use sqlite_model_repository::SqliteModelRepository;
pub use sqlite_settings_repository::SqliteSettingsRepository;
