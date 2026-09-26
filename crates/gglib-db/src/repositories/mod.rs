#![doc = include_str!("README.md")]
mod model_files_repository;
#[allow(
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod row_mappers;
mod sqlite_benchmark_repository;
#[allow(
    clippy::cast_possible_wrap,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod sqlite_chat_history_repository;
pub(crate) mod sqlite_loop_guard_trip_log;
#[allow(
    clippy::needless_pass_by_value,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod sqlite_mcp_repository;
#[allow(
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) mod sqlite_model_repository;
mod sqlite_settings_repository;

pub use model_files_repository::ModelFilesRepository;
pub use sqlite_benchmark_repository::SqliteBenchmarkRepository;
pub use sqlite_chat_history_repository::SqliteChatHistoryRepository;
pub use sqlite_loop_guard_trip_log::SqliteLoopGuardTripLog;
pub use sqlite_mcp_repository::SqliteMcpRepository;
pub use sqlite_model_repository::SqliteModelRepository;
pub use sqlite_settings_repository::SqliteSettingsRepository;
