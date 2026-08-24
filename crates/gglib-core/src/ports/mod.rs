#![doc = include_str!("README.md")]
pub(crate) mod agent;
pub(crate) mod benchmark;
pub mod chat_history;
pub(crate) mod download;
pub(crate) mod download_manager;
pub(crate) mod event_emitter;
pub(crate) mod gguf_parser;
pub mod huggingface;
pub(crate) mod llm_completion;
pub(crate) mod mcp_dto;
pub(crate) mod mcp_error;
pub(crate) mod mcp_repository;
pub mod model_catalog;
pub(crate) mod model_registrar;
pub(crate) mod model_repository;
pub mod model_runtime;
pub(crate) mod process_runner;
pub(crate) mod retry_observer;
pub(crate) mod server_health;
pub(crate) mod server_log_sink;
pub(crate) mod settings_repository;
pub(crate) mod system_probe;
pub(crate) mod tool_executor_filter;
pub(crate) mod tool_support;
pub(crate) mod usage_sink;

use std::sync::Arc;
use thiserror::Error;

// Re-export agent port types for convenience
pub use agent::{AgentError, AgentLoopPort, AgentRunOutput, ToolExecutorPort};
// Re-export LLM completion port (LlmStreamEvent lives in domain::agent)
pub use llm_completion::LlmCompletionPort;
// Re-export tool-executor filter decorators
pub use tool_executor_filter::{EmptyToolExecutor, FilteredToolExecutor, TOOL_NOT_AVAILABLE_MSG};

// Re-export repository traits for convenience
pub use benchmark::BenchmarkRepositoryPort;
pub use chat_history::{ChatHistoryError, ChatHistoryRepository};
pub use download::{QuantizationResolver, Resolution, ResolvedFile};
pub use download_manager::{DownloadManagerConfig, DownloadManagerPort, DownloadRequest};
pub use event_emitter::{AppEventEmitter, NoopEmitter};
pub use gguf_parser::{
    GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, NoopGgufParser,
};
pub use huggingface::{
    HfClientPort, HfFileInfo, HfPortError, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult,
};
pub use mcp_dto::{ResolutionAttempt, ResolutionStatus};
pub use mcp_error::McpServiceError;
pub use mcp_repository::{McpRepositoryError, McpServerRepository};
pub use model_catalog::{CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelSummary};
pub use model_registrar::{CompletedDownload, ModelRegistrarPort};
pub use model_repository::ModelRepository;
pub use model_runtime::{
    Admission, AdmissionLease, AdmissionRelease, LaunchOverrides, ModelRuntimeError,
    ModelRuntimePort, NoopModelRuntime, PinnedSpec, RunningTarget, RuntimeErrorEnvelope,
};
pub use process_runner::{JinjaMode, ProcessHandle, ServerConfig};
pub use retry_observer::RetryObserver;
pub use server_health::ServerHealthStatus;
pub use server_log_sink::ServerLogSinkPort;
pub use settings_repository::SettingsRepository;
pub use system_probe::SystemProbePort;
pub use tool_support::{
    ModelSource, ToolFormat, ToolSupportDetection, ToolSupportDetectionInput,
    ToolSupportDetectorPort,
};
pub use usage_sink::UsageSink;

/// Container for all repository trait objects.
///
/// This struct provides a consistent way to wire repositories across adapters
/// without coupling them to concrete implementations. It lives in `gglib-core`
/// so that `AppCore` can accept it without depending on `gglib-db`.
///
/// # Example
///
/// ```ignore
/// // In gglib-db factory:
/// pub fn build_repos(pool: &SqlitePool) -> Repos { ... }
///
/// // In adapter bootstrap:
/// let repos = gglib_db::factory::build_repos(&pool);
/// let core = AppCore::new(repos);
/// ```
#[derive(Clone)]
pub struct Repos {
    /// Model repository for CRUD operations on models.
    pub models: Arc<dyn ModelRepository>,
    /// Settings repository for application settings.
    pub settings: Arc<dyn SettingsRepository>,
    /// MCP server repository for MCP server configurations.
    pub mcp_servers: Arc<dyn McpServerRepository>,
    /// Chat history repository for conversations and messages.
    pub chat_history: Arc<dyn ChatHistoryRepository>,
}

impl Repos {
    /// Create a new Repos container.
    pub fn new(
        models: Arc<dyn ModelRepository>,
        settings: Arc<dyn SettingsRepository>,
        mcp_servers: Arc<dyn McpServerRepository>,
        chat_history: Arc<dyn ChatHistoryRepository>,
    ) -> Self {
        Self {
            models,
            settings,
            mcp_servers,
            chat_history,
        }
    }
}

/// Domain-specific errors for repository operations.
///
/// This error type abstracts away storage implementation details (e.g., sqlx errors)
/// and provides a clean interface for services to handle storage failures.
#[derive(Debug, Error)]
pub enum RepositoryError {
    /// The requested entity was not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// An entity with the same identifier already exists.
    #[error("Already exists: {0}")]
    AlreadyExists(String),

    /// Storage backend error (database, filesystem, etc.).
    #[error("Storage error: {0}")]
    Storage(String),

    /// Serialization or deserialization failed.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// A constraint was violated (e.g., foreign key, unique constraint).
    #[error("Constraint violation: {0}")]
    Constraint(String),
}

/// Core error type for semantic domain errors.
///
/// This is the canonical error type used across the core domain.
/// Adapters should map this to their own error types (HTTP status codes,
/// CLI exit codes, Tauri serialized errors).
#[derive(Debug, Error)]
pub enum CoreError {
    /// Repository operation failed.
    #[error(transparent)]
    Repository(#[from] RepositoryError),

    /// Settings validation error.
    #[error(transparent)]
    Settings(#[from] crate::settings::SettingsError),

    /// Validation error (invalid input).
    #[error("Validation error: {0}")]
    Validation(String),
}
