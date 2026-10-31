#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

mod daemon_startup;
mod database_file;
pub mod factory;
mod loop_guard_trip_writer;
pub mod repositories;
pub mod setup;
mod sqlite_init;

// Re-export factory for convenient access
pub use factory::CoreFactory;

// Re-export repository implementations
pub use repositories::{
    ModelFilesRepository, SqliteAttachmentStore, SqliteBenchmarkRepository,
    SqliteChatHistoryRepository, SqliteLoopGuardTripLog, SqliteMcpRepository,
    SqliteModelRepository, SqliteSettingsRepository,
};

// The loop guard's batched writer: the sink the proxy records into.
pub use loop_guard_trip_writer::{LoopGuardTripWriter, TripWriterLimits};

// What the daemon, and only the daemon, runs once when it starts.
pub use daemon_startup::repair_at_daemon_start;

// Re-export setup functions for convenient access
pub use setup::setup_database;
#[cfg(any(test, feature = "test-utils"))]
pub use setup::setup_test_database;
