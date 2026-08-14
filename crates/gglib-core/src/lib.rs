#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unused_crate_dependencies)]

pub mod access;
pub mod cache_config;
pub mod cache_metrics;
pub mod contracts;
pub(crate) mod cors;
pub mod debug_switches;
pub mod domain;
pub mod download;
pub mod events;
pub mod is_local_origin;
pub mod normalize;
pub mod paths;
pub mod ports;
pub mod request_pipeline;
pub mod retry;
pub mod server_config;
pub mod services;
pub mod settings;
pub mod sse;
pub mod telemetry;
pub mod utils;

// Re-export commonly used types for convenience
pub use access::{ApiKeySource, ProxyAccessConfig};
pub use cors::CorsConfig;
pub use domain::{
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentConfigError, AgentEvent, AgentMessage,
    AssistantContent, ChatMessage, Conversation, ConversationUpdate, DEFAULT_MAX_ITERATIONS,
    DEFAULT_MAX_PARALLEL_TOOLS, DEFAULT_MAX_STAGNATION_STEPS, LlmStreamEvent, LoopDetector,
    MAX_ITERATIONS_CEILING, MAX_PARALLEL_TOOLS_CEILING, MAX_STAGNATION_STEPS_CEILING,
    MAX_TOOL_TIMEOUT_MS_CEILING, MIN_CONTEXT_BUDGET_CHARS, MIN_TOOL_TIMEOUT_MS, McpEnvEntry,
    McpLifecycle, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, Message, MessageContent, MessageRole, Model, ModelCapabilities,
    ModelFilterOptions, NameSource, NewConversation, NewMcpServer, NewMessage, NewModel,
    RangeValues, SEARCH_RESULTS_CAP, StagnationDetector, ToolCall, ToolDefinition, ToolIndex,
    ToolResult, ToolSummary, UNKNOWN_MODEL_NAME, UpdateMcpServer, capabilities_from_architecture,
    declared_name, infer_from_chat_template, is_observation_batch, repo_short_name,
    resolve_model_name, strip_gguf_suffix, transform_messages_for_capabilities,
};
pub use download::{
    AttemptCounts, CompletionDetail, CompletionKey, CompletionKind, DownloadError, DownloadEvent,
    DownloadId, DownloadResult, DownloadStatus, DownloadSummary, FailedDownload, Quantization,
    QueueRunSummary, QueueSnapshot, QueuedDownload, ShardInfo,
};
pub use events::{AppEvent, McpServerSummary, ModelSummary, ServerSnapshotEntry};
pub use ports::{
    AgentError, AgentLoopPort, AgentRunOutput, AppEventBridge, AppEventEmitter, ChatHistoryError,
    ChatHistoryRepository, CompletedDownload, CoreError, DownloadEventEmitterPort,
    DownloadManagerConfig, DownloadManagerPort, DownloadRequest, DownloadStateRepositoryPort,
    EmptyToolExecutor, FilteredToolExecutor, GgufCapabilities, GgufMetadata, GgufParseError,
    GgufParserPort, HfClientPort, HfFileInfo, HfPortError, HfQuantInfo, HfRepoInfo,
    HfSearchOptions, HfSearchResult, LlmCompletionPort, McpErrorCategory, McpErrorInfo,
    McpRepositoryError, McpServerRepository, McpServiceError, ModelRegistrarPort, ModelRepository,
    NoopDownloadEmitter, NoopEmitter, NoopGgufParser, ProcessError, ProcessHandle, ProcessRunner,
    QuantizationResolver, Repos, RepositoryError, Resolution, ResolvedFile, ServerConfig,
    SettingsRepository, TOOL_NOT_AVAILABLE_MSG, ToolExecutorPort, UsageSink,
};
pub use services::{ChatHistoryService, ModelRegistrar};
pub use settings::{
    DAEMON_PORT, DEFAULT_CONTEXT_SIZE, DEFAULT_LLAMA_BASE_PORT, DEFAULT_PROXY_PORT, Settings,
    SettingsError, SettingsUpdate, validate_settings,
};

// Re-export origin validation utility
pub use is_local_origin::is_local_origin;

// Re-export timing utility
pub use utils::timing::{elapsed_ms, format_duration_human};

// Re-export path utilities
#[cfg(not(target_os = "windows"))]
pub use paths::DEFAULT_MODELS_DIR_RELATIVE;
pub use paths::{
    DirectoryCreationStrategy, ModelsDirResolution, ModelsDirSource, PathError, data_root,
    database_path, default_models_dir, ensure_directory, env_file_path, is_prebuilt_binary,
    llama_config_path, llama_cpp_dir, llama_server_path, persist_env_value, persist_models_dir,
    resolve_models_dir, resource_root, verify_writable,
};

// Silence unused dev-dependency warnings until we add mock-based tests
#[cfg(test)]
use mockall as _;
#[cfg(test)]
use tokio_test as _;
