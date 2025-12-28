#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unused_crate_dependencies)]

pub mod contracts;
pub mod domain;
pub mod download;
pub mod events;
pub mod paths;
pub mod ports;
pub mod services;
pub mod settings;
pub mod utils;

// Re-export commonly used types for convenience
pub use domain::{
    ChatMessage, Conversation, ConversationUpdate, McpEnvEntry, McpServer, McpServerConfig,
    McpServerStatus, McpServerType, McpTool, McpToolResult, Message, MessageRole, Model,
    ModelCapabilities, ModelFilterOptions, NewConversation, NewMcpServer, NewMessage, NewModel,
    RangeValues, UpdateMcpServer, infer_from_chat_template, transform_messages_for_capabilities,
};
pub use download::{
    AttemptCounts, CompletionDetail, CompletionKey, CompletionKind, DownloadError, DownloadEvent,
    DownloadId, DownloadResult, DownloadStatus, DownloadSummary, FailedDownload, Quantization,
    QueueRunSummary, QueueSnapshot, QueuedDownload, ShardInfo,
};
pub use events::{AppEvent, McpServerSummary, ModelSummary, ServerSnapshotEntry};
pub use ports::{
    AppEventBridge, AppEventEmitter, ChatHistoryError, ChatHistoryRepository, CompletedDownload,
    CoreError, DownloadEventEmitterPort, DownloadManagerConfig, DownloadManagerPort,
    DownloadRequest, DownloadStateRepositoryPort, GgufCapabilities, GgufMetadata, GgufParseError,
    GgufParserPort, HfClientPort, HfFileInfo, HfPortError, HfQuantInfo, HfRepoInfo,
    HfSearchOptions, HfSearchResult, McpErrorCategory, McpErrorInfo, McpRepositoryError,
    McpServerRepository, McpServiceError, ModelRegistrarPort, ModelRepository, NoopDownloadEmitter,
    NoopEmitter, NoopGgufParser, ProcessError, ProcessHandle, ProcessRunner, QuantizationResolver,
    Repos, RepositoryError, Resolution, ResolvedFile, ServerConfig, ServerHealth,
    SettingsRepository,
};
pub use services::{ChatHistoryService, ModelRegistrar};
pub use settings::{
    DEFAULT_LLAMA_BASE_PORT, DEFAULT_PROXY_PORT, Settings, SettingsError, SettingsUpdate,
    validate_settings,
};

// Re-export path utilities
pub use paths::{
    DEFAULT_MODELS_DIR_RELATIVE, DirectoryCreationStrategy, ModelsDirResolution, ModelsDirSource,
    PathError, data_root, database_path, default_models_dir, ensure_directory, env_file_path,
    is_prebuilt_binary, llama_cli_path, llama_config_path, llama_cpp_dir, llama_server_path,
    persist_env_value, persist_models_dir, resolve_models_dir, resource_root, verify_writable,
};

// Silence unused dev-dependency warnings until we add mock-based tests
#[cfg(test)]
use mockall as _;
#[cfg(test)]
use tokio_test as _;
