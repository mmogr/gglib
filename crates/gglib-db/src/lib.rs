#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]

mod database_file;
pub mod factory;
mod loop_guard_trip_writer;
pub mod repositories;
pub mod setup;

// Re-export factory for convenient access
pub use factory::CoreFactory;

// Re-export repository implementations
pub use repositories::{
    ModelFilesRepository, SqliteBenchmarkRepository, SqliteChatHistoryRepository,
    SqliteLoopGuardTripLog, SqliteMcpRepository, SqliteModelRepository, SqliteSettingsRepository,
};

// The loop guard's batched writer: the sink the proxy records into.
pub use loop_guard_trip_writer::{LoopGuardTripWriter, TripWriterLimits};

// Re-export setup functions for convenient access
pub use setup::cleanup_zombie_benchmark_runs;
pub use setup::setup_database;
#[cfg(any(test, feature = "test-utils"))]
pub use setup::setup_test_database;
