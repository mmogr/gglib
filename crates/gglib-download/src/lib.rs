//! Download manager for gglib.
//!
//! This crate provides the download subsystem for gglib, handling model downloads
//! from HuggingFace Hub with queuing, progress tracking, and cancellation support.
//!
//! # Architecture
//!
//! The download manager implements `gglib_core::ports::DownloadManagerPort` and
//! is injected with its dependencies via `DownloadManagerDeps`:
//!
//! - `ModelRegistrarPort` - For registering completed downloads
//! - `DownloadStateRepositoryPort` - For persisting queue state
//! - `HfClientPort` - For HuggingFace API access
//!
//! # Usage
//!
//! ```ignore
//! use gglib_download::{build_download_manager, DownloadManagerDeps};
//!
//! let deps = DownloadManagerDeps {
//!     model_registrar,
//!     download_repo,
//!     hf_client,
//!     config,
//! };
//!
//! let manager = build_download_manager(deps);
//! // manager implements DownloadManagerPort
//! ```
//!
//! # Modules
//!
//! - `queue` - Pure state machine for download queue management
//! - `progress` - Progress tracking and throttling
//! - `executor` - Download execution (Python subprocess)
//! - `resolver` - HuggingFace file resolution

// Re-export core types for convenience
pub use gglib_core::download::{
    DownloadError, DownloadEvent, DownloadId, DownloadStatus, FailedDownload, QueueSnapshot,
    QueuedDownload, Quantization,
};
pub use gglib_core::ports::{
    CompletedDownload, DownloadManagerConfig, DownloadManagerPort, DownloadRequest,
    DownloadStateRepositoryPort, ModelRegistrarPort,
};

// Internal modules (pub(crate) to keep implementation private)
pub(crate) mod executor;
pub(crate) mod progress;
pub(crate) mod queue;
pub(crate) mod resolver;

// Public API
mod manager;

pub use manager::{build_download_manager, DownloadManagerDeps, DownloadManagerImpl};
