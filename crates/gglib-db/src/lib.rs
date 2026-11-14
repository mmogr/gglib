#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

/// SQL that is true when a `models` row holds llama-server's tool answer
/// (ADR 0007): its `template_caps` is JSON whose `supports_tool_calls` is
/// `true` or `false`. The re-registration upsert and library step 10 both
/// ask it; `Model::engine_tool_answer` is the same test in Rust, since a
/// non-boolean there reads as no caps at all.
macro_rules! holds_engine_tool_answer {
    () => {
        "(CASE WHEN json_valid(models.template_caps) \
            THEN json_type(models.template_caps, '$.supports_tool_calls') IN ('true', 'false') \
            ELSE 0 END)"
    };
}

mod daemon_startup;
mod database_file;
pub mod factory;
mod header_facts_pass;
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
pub use header_facts_pass::fill_header_facts_once;

// Re-export setup functions for convenient access
pub use setup::{backup_beside, setup_database};
#[cfg(any(test, feature = "test-utils"))]
pub use setup::{setup_test_database, write_newer_database};
