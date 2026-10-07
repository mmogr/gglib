#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
//! - `resolver` - `HuggingFace` file resolution

// Re-export core types for convenience
pub use gglib_core::download::{
    DownloadError, DownloadEvent, DownloadId, DownloadOutcome, DownloadPhase, DownloadRow,
    FinishedDownload, Quantization, QueueSnapshot, ShardInfo,
};
pub use gglib_core::ports::{
    CompletedDownload, DownloadManagerConfig, DownloadManagerPort, DownloadRequest,
    ModelRegistrarPort,
};

// Internal modules (pub(crate) to keep implementation private)
pub(crate) mod executor;
mod meter;
pub(crate) mod queue;
mod resolver;

// Quantization selection service
mod quant_selector;

// CLI execution module (list_quantizations + Python bridge helpers)
pub mod cli_exec;

// Public API - modular download manager
mod manager;

#[cfg(test)]
mod test_hub;

pub use manager::{DownloadManagerDeps, build_download_manager};
