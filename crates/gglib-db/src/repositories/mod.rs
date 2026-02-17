//! Repository implementations using `SQLite`.
//!
//! These implementations encapsulate all SQL queries and database access.
//! The `SqlitePool` is confined to this module and never exposed through
//! the port trait signatures.

mod model_files_repository;
mod row_mappers;
mod sqlite_chat_history_repository;
mod sqlite_download_state_repository;
mod sqlite_mcp_repository;
mod sqlite_model_repository;
mod sqlite_settings_repository;

pub use model_files_repository::ModelFilesRepository;
pub use sqlite_chat_history_repository::SqliteChatHistoryRepository;
pub use sqlite_download_state_repository::SqliteDownloadStateRepository;
pub use sqlite_mcp_repository::SqliteMcpRepository;
pub use sqlite_model_repository::SqliteModelRepository;
pub use sqlite_settings_repository::SqliteSettingsRepository;
