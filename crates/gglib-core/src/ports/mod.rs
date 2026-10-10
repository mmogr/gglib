#![doc = include_str!("README.md")]
pub(crate) mod agent;
pub(crate) mod agent_guard_sink;
pub mod attachment_store;
pub(crate) mod benchmark;
pub mod chat_history;
pub(crate) mod download;
pub(crate) mod download_manager;
#[cfg(any(test, feature = "test-utils"))]
mod download_manager_fixture;
pub(crate) mod event_emitter;
pub(crate) mod generation_gate;
pub(crate) mod gguf_parser;
pub(crate) mod hub_chats;
pub mod huggingface;
pub(crate) mod image_generation;
mod jinja_mode;
pub(crate) mod llm_completion;
pub(crate) mod loop_guard_trips;
pub(crate) mod mcp_dto;
pub(crate) mod mcp_error;
pub(crate) mod mcp_repository;
pub mod model_catalog;
#[cfg(any(test, feature = "test-utils"))]
mod model_catalog_fixture;
pub(crate) mod model_files;
pub(crate) mod model_registrar;
pub(crate) mod model_repository;
pub mod model_runtime;
#[cfg(any(test, feature = "test-utils"))]
mod model_summary_fixture;
pub(crate) mod pinned;
pub(crate) mod process_runner;
pub(crate) mod remote_gateway;
pub(crate) mod retry_observer;
pub(crate) mod runs;
pub(crate) mod server_health;
#[cfg(any(test, feature = "test-utils"))]
mod settings_fixture;
pub(crate) mod settings_repository;
pub(crate) mod system_probe;
pub(crate) mod tool_executor_filter;
pub(crate) mod tool_support;
pub(crate) mod usage_sink;

use std::sync::Arc;
use thiserror::Error;

// Re-export agent port types for convenience
pub use agent::{AgentError, AgentLoopPort, AgentRunOutput, ToolExecutorPort};
pub use agent_guard_sink::{AgentGuardReporter, AgentGuardSink};
// Re-export LLM completion port (LlmStreamEvent lives in domain::agent)
pub use llm_completion::LlmCompletionPort;
pub use loop_guard_trips::LoopGuardTripSink;
// Re-export tool-executor filter decorators
pub use tool_executor_filter::{EmptyToolExecutor, FilteredToolExecutor, TOOL_NOT_AVAILABLE_MSG};

// Re-export repository traits for convenience
pub use attachment_store::{AttachmentError, AttachmentStore};
pub use benchmark::BenchmarkRepositoryPort;
pub use chat_history::{ChatHistoryError, ChatHistoryRepository};
pub use download::{QuantizationResolver, Resolution, ResolvedFile};
pub use download_manager::{DownloadManagerConfig, DownloadManagerPort};
#[cfg(any(test, feature = "test-utils"))]
pub use download_manager_fixture::AskedDownloads;
pub use event_emitter::{AppEventEmitter, NoopEmitter};
pub use generation_gate::{
    GateError, GateRelease, GateWait, GateWaitObserver, GenerationGate, GenerationTurn, TurnKind,
    WaitReason,
};
pub use gguf_parser::{
    GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, NoopGgufParser, TensorTable,
};
pub use hub_chats::{AgentRunStarter, HubChatsError, HubChatsPort, TurnRefused};
pub use huggingface::{
    HfClientPort, HfFileInfo, HfModelKind, HfPortError, HfQuantInfo, HfRepoInfo, HfSearchOptions,
    HfSearchResult, HfSortField, SNIFF_HEAD_BYTES,
};
pub use image_generation::{
    GeneratedImage, IMAGE_JOB_DEADLINE, ImageBatch, ImageError, ImageGenerationPort, ImageProgress,
    ImageRequest, ImageSize, ImageStage, MAX_IMAGES_PER_REQUEST, UnreadableSize,
};
pub use jinja_mode::JinjaMode;
pub use mcp_dto::{ResolutionAttempt, ResolutionStatus};
pub use mcp_error::McpServiceError;
pub use mcp_repository::{McpRepositoryError, McpServerRepository};
pub use model_catalog::{CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelSummary};
#[cfg(any(test, feature = "test-utils"))]
pub use model_catalog_fixture::NamedCatalog;
pub use model_files::ModelFilesRepositoryPort;
pub use model_registrar::{CompletedDownload, ModelRegistrarPort, RegisteredDownload};
pub use model_repository::ModelRepository;
pub use model_runtime::{
    Admission, AdmissionLease, AdmissionRelease, AdmitObserver, LaunchOverrides, ModelRuntimeError,
    ModelRuntimePort, NoopModelRuntime, PinnedSpec, RunningTarget, RuntimeErrorEnvelope,
};
pub use process_runner::{ProcessHandle, ServerConfig};
pub use remote_gateway::RemoteGatewayPort;
pub use retry_observer::RetryObserver;
pub use runs::{Created, RunEvent, RunEvents, RunScope, RunsError, RunsPort};
pub use server_health::ServerHealthStatus;
#[cfg(any(test, feature = "test-utils"))]
pub use settings_fixture::InMemorySettings;
pub use settings_repository::{SettingsChange, SettingsRepository};
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
/// // In gglib-db:
/// impl CoreFactory {
///     pub fn build_repos(pool: SqlitePool) -> Repos { ... }
/// }
///
/// // In gglib-bootstrap:
/// let repos = gglib_db::CoreFactory::build_repos(pool);
/// let core = AppCore::new(repos, hf_client, downloads);
/// ```
#[derive(Clone)]
pub struct Repos {
    /// Model repository for CRUD operations on models.
    pub models: Arc<dyn ModelRepository>,
    /// Model files repository for the per-file rows of each model.
    pub model_files: Arc<dyn ModelFilesRepositoryPort>,
    /// Settings repository for application settings.
    pub settings: Arc<dyn SettingsRepository>,
    /// MCP server repository for MCP server configurations.
    pub mcp_servers: Arc<dyn McpServerRepository>,
    /// Chat history repository for conversations and messages.
    pub chat_history: Arc<dyn ChatHistoryRepository>,
    /// Attachment store for the images messages carry.
    pub attachments: Arc<dyn AttachmentStore>,
}

impl Repos {
    /// Create a new Repos container.
    pub fn new(
        models: Arc<dyn ModelRepository>,
        model_files: Arc<dyn ModelFilesRepositoryPort>,
        settings: Arc<dyn SettingsRepository>,
        mcp_servers: Arc<dyn McpServerRepository>,
        chat_history: Arc<dyn ChatHistoryRepository>,
        attachments: Arc<dyn AttachmentStore>,
    ) -> Self {
        Self {
            models,
            model_files,
            settings,
            mcp_servers,
            chat_history,
            attachments,
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
