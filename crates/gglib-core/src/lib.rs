//! Core domain types and port definitions for gglib.
//!
//! This crate contains the pure domain logic, traits (ports), and domain types
//! that form the heart of the application. No infrastructure dependencies (sqlx,
//! filesystem, process management) should appear here.
//!
//! # Structure
//!
//! - `domain` - Core domain types (`Model`, `NewModel`, server configuration)
//! - `ports` - Trait definitions for repositories and external systems
//! - `events` - Canonical event union for all cross-adapter events
//!
//! # Design Rules
//!
//! - No adapter-specific dependencies (sqlx, clap, tauri, axum)
//! - All external interactions defined via trait ports
//! - Error types are semantic, not representational

#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

pub mod domain;
pub mod download;
pub mod events;
pub mod ports;
pub mod services;
pub mod settings;

// Re-export commonly used types for convenience
pub use domain::{
    McpEnvEntry, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, Model, NewMcpServer, NewModel,
};
pub use download::{
    DownloadError, DownloadEvent, DownloadId, DownloadResult, DownloadStatus, DownloadSummary,
    FailedDownload, Quantization, QueueSnapshot, QueuedDownload, ShardInfo,
};
pub use events::{AppEvent, McpServerSummary, ModelSummary, ServerSnapshotEntry};
pub use ports::{
    AppEventEmitter, CoreError, HfClientPort, HfFileInfo, HfPortError, HfQuantInfo, HfRepoInfo,
    HfSearchOptions, HfSearchResult, McpErrorCategory, McpErrorInfo, McpRepositoryError,
    McpServerRepository, McpServiceError, ModelRepository, NoopEmitter, ProcessError,
    ProcessHandle, ProcessRunner, QuantizationResolver, Repos, RepositoryError, Resolution,
    ResolvedFile, ServerConfig, ServerHealth, SettingsRepository,
};
pub use settings::{Settings, SettingsError, SettingsUpdate, validate_settings};

// Silence unused dev-dependency warnings until we add mock-based tests
#[cfg(test)]
use mockall as _;
#[cfg(test)]
use tokio_test as _;
