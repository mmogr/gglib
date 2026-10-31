#![doc = include_str!("README.md")]
mod branch_rows;
#[cfg(test)]
mod chat_fixture;
mod message_rows;
mod model_component_rows;
mod model_files_repository;
mod row_mappers;
mod sqlite_attachment_store;
mod sqlite_benchmark_repository;
mod sqlite_chat_history_repository;
pub(crate) mod sqlite_loop_guard_trip_log;
mod sqlite_mcp_repository;
pub(crate) mod sqlite_model_repository;
pub(crate) mod sqlite_settings_repository;

pub use model_files_repository::ModelFilesRepository;
pub use sqlite_attachment_store::SqliteAttachmentStore;
pub use sqlite_benchmark_repository::SqliteBenchmarkRepository;
pub use sqlite_chat_history_repository::SqliteChatHistoryRepository;
pub use sqlite_loop_guard_trip_log::SqliteLoopGuardTripLog;
pub use sqlite_mcp_repository::SqliteMcpRepository;
pub use sqlite_model_repository::SqliteModelRepository;
pub use sqlite_settings_repository::SqliteSettingsRepository;
