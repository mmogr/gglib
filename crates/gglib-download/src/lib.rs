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
pub(crate) mod progress;
pub(crate) mod queue;
mod resolver;

// Re-export the progress throttle for consumers (adapters). Used by the Tauri
// llama.cpp installer, whose progress callback is driven by raw HTTP chunks
// rather than a fixed tick and so needs its own emission rate limit.
pub use progress::ProgressThrottle;

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
