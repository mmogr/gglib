//! Port definitions (trait abstractions) for external systems.
//!
//! Ports define the interfaces that the core domain expects from infrastructure.
//! They contain no implementation details and use only domain types.
//!
//! # Design Rules
//!
//! - No `sqlx` types in any signature
//! - No process/filesystem implementation details
//! - Traits are minimal and CRUD-focused for repositories
//! - Intent-based methods for process runner (not implementation-leaking)

pub mod chat_history;
pub mod download;
pub mod download_event_emitter;
pub mod download_manager;
pub mod download_state;
pub mod event_emitter;
pub mod gguf_parser;
pub mod huggingface;
pub mod mcp_dto;
pub mod mcp_error;
pub mod mcp_repository;
pub mod model_catalog;
pub mod model_registrar;
pub mod model_repository;
pub mod model_runtime;
pub mod process_runner;
pub mod server_health;
pub mod server_log_sink;
pub mod settings_repository;
pub mod system_probe;
pub mod tool_support;
pub mod voice;

use std::sync::Arc;
use thiserror::Error;

// Re-export repository traits for convenience
pub use chat_history::{ChatHistoryError, ChatHistoryRepository};
pub use download::{QuantizationResolver, Resolution, ResolvedFile};
pub use download_event_emitter::{AppEventBridge, DownloadEventEmitterPort, NoopDownloadEmitter};
pub use download_manager::{DownloadManagerConfig, DownloadManagerPort, DownloadRequest};
pub use download_state::DownloadStateRepositoryPort;
pub use event_emitter::{AppEventEmitter, NoopEmitter};
pub use gguf_parser::{
    GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, NoopGgufParser,
};
pub use huggingface::{
    HfClientPort, HfFileInfo, HfPortError, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult,
};
pub use mcp_dto::{ResolutionAttempt, ResolutionStatus};
pub use mcp_error::{McpErrorCategory, McpErrorInfo, McpServiceError};
pub use mcp_repository::{McpRepositoryError, McpServerRepository};
pub use model_catalog::{CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelSummary};
pub use model_registrar::{CompletedDownload, ModelRegistrarPort};
pub use model_repository::ModelRepository;
pub use model_runtime::{ModelRuntimeError, ModelRuntimePort, RunningTarget};
pub use process_runner::{ProcessHandle, ProcessRunner, ServerConfig, ServerHealth};
pub use server_health::ServerHealthStatus;
pub use server_log_sink::ServerLogSinkPort;
pub use settings_repository::SettingsRepository;
pub use system_probe::{SystemProbeError, SystemProbePort, SystemProbeResult};
pub use tool_support::{
    ModelSource, ToolFormat, ToolSupportDetection, ToolSupportDetectionInput,
    ToolSupportDetectorPort,
};
pub use voice::{
    AudioDeviceDto, SttModelInfoDto, TtsModelInfoDto, VoiceInfoDto, VoiceModelsDto,
    VoicePipelinePort, VoicePortError, VoiceStatusDto,
};

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
/// let core = AppCore::new(repos, runner);
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

/// Domain-specific errors for process runner operations.
///
/// This error type abstracts away process management implementation details
/// and provides a clean interface for services to handle process failures.
#[derive(Debug, Error)]
pub enum ProcessError {
    /// Failed to start the process.
    #[error("Failed to start: {0}")]
    StartFailed(String),

    /// Failed to stop the process.
    #[error("Failed to stop: {0}")]
    StopFailed(String),

    /// The process is not running.
    #[error("Process not running: {0}")]
    NotRunning(String),

    /// Health check failed.
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Resource exhaustion (e.g., no available ports).
    #[error("Resource exhaustion: {0}")]
    ResourceExhausted(String),

    /// Internal process error.
    #[error("Internal error: {0}")]
    Internal(String),
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

    /// Process operation failed.
    #[error(transparent)]
    Process(#[from] ProcessError),

    /// Settings validation error.
    #[error(transparent)]
    Settings(#[from] crate::settings::SettingsError),

    /// Validation error (invalid input).
    #[error("Validation error: {0}")]
    Validation(String),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// External service error.
    #[error("External service error: {0}")]
    ExternalService(String),

    /// Internal error (unexpected condition).
    #[error("Internal error: {0}")]
    Internal(String),
}
