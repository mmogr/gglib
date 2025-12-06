#![doc = include_str!(concat!(env!("OUT_DIR"), "/crate_docs.md"))]

pub mod cli;
pub mod commands;
pub mod core;
pub mod download;
pub mod gguf;
pub mod infra;
pub mod models;
pub mod proxy;
pub mod services;
pub mod utils;

// Re-export specific commonly used types
pub use download::{
    DownloadError, DownloadEvent, DownloadId, DownloadManager, DownloadManagerConfig,
    DownloadRequest, DownloadStatus, DownloadSummary, Quantization, QueueSnapshot,
};
pub use gguf::{
    GgufError, GgufMetadata, GgufResult, GgufValue, ReasoningDetection, ToolCallingDetection,
    apply_capability_detection, detect_reasoning_support, detect_tool_support, parse_gguf_file,
};
pub use models::Gguf;
pub use models::gui::{ApiResponse, GuiModel, StartServerRequest, StartServerResponse};
pub use services::{database, gui_backend};
pub use utils::{input, validation};

// Re-export core domain types and ports (Phase 2 architecture)
pub use core::{
    AppEvent, Model, ModelRepository, NewModel, ProcessError, ProcessHandle, ProcessRunner,
    RepositoryError, ServerConfig, ServerHealth, SettingsRepository,
};

// Re-export infra implementations
pub use infra::{LlamaProcessRunner, SqliteModelRepository, SqliteSettingsRepository};
