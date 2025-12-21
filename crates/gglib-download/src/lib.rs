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

// Re-export progress throttle for consumers (adapters)
pub use progress::ProgressThrottle;

// Re-export queue types needed by consumers
pub use queue::ShardGroupId;

// Re-export resolver for CLI usage
pub use resolver::HfQuantizationResolver;

// Quantization selection service
mod quant_selector;
pub use quant_selector::{QuantizationSelection, QuantizationSelector, SelectionError};

// CLI execution module (transitional - synchronous CLI downloads)
pub mod cli_exec;

// Public API - modular download manager
mod manager;

pub use manager::{
    CompletedJob, DownloadDestination, DownloadJob, DownloadManagerDeps, DownloadManagerImpl,
    ProgressUpdate, QueueAutoResult, WorkerDeps, build_download_manager,
};
