#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
//! - `resolver` - `HuggingFace` file resolution

// Re-export core types for convenience
pub use gglib_core::download::{
    DownloadError, DownloadEvent, DownloadId, DownloadStatus, DownloadSummary, FailedDownload,
    Quantization, QueueSnapshot, QueuedDownload, ShardInfo,
};
pub use gglib_core::ports::{
    CompletedDownload, DownloadManagerConfig, DownloadManagerPort, DownloadRequest,
    DownloadStateRepositoryPort, ModelRegistrarPort,
};

// Internal modules (pub(crate) to keep implementation private)
pub(crate) mod executor;
pub(crate) mod queue;
mod resolver;

// Quantization selection service
mod quant_selector;

// CLI execution module (list_quantizations + Python bridge helpers)
pub mod cli_exec;

// CLI terminal progress emitter
mod cli_emitter;
pub use cli_emitter::{CliDownloadEventEmitter, rate_suffix, total_bytes_key};

// Public API - modular download manager
mod manager;

pub use manager::{DownloadManagerDeps, build_download_manager};
