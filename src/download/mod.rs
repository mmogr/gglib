//! Download domain module.
//!
//! This module provides a clean, opinionated API for downloading models from HuggingFace Hub.
//! It uses a Python-based executor (via `hf_xet`) for fast, resumable downloads.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    DownloadManager                          │
//! │  (thin orchestrator: queue, executor, progress emission)    │
//! └─────────────────────────────────────────────────────────────┘
//!          │                    │                    │
//!          ▼                    ▼                    ▼
//! ┌────────────────┐  ┌──────────────────┐  ┌───────────────┐
//! │  DownloadQueue │  │ PythonExecutor   │  │ ProgressEmitter│
//! │  (pure state)  │  │ (I/O boundary)   │  │ (I/O boundary) │
//! └────────────────┘  └──────────────────┘  └───────────────┘
//! ```
//!
//! # Domain Types
//!
//! - [`DownloadId`] - Canonical identifier for a download (`model_id:quantization`)
//! - [`DownloadRequest`] - Request parameters for starting a download
//! - [`DownloadEvent`] - Discriminated union of all download state changes
//! - [`QueueSnapshot`] - Current state of the download queue
//!
//! # Usage
//!
//! ```rust,ignore
//! use gglib::download::{DownloadManager, DownloadId};
//!
//! let manager = DownloadManager::new();
//!
//! // Queue a download
//! let id = DownloadId::new("unsloth/Llama-3", Some("Q4_K_M"));
//! let position = manager.queue(id).await?;
//!
//! // Start processing with event callback
//! manager.run(|event| {
//!     println!("Event: {:?}", event);
//! }).await;
//! ```

pub mod domain;
pub mod executor;
pub mod huggingface;
pub mod progress;
pub mod queue;
mod service;

// Re-export public API
pub use domain::errors::DownloadError;
pub use domain::events::{DownloadEvent, DownloadStatus, DownloadSummary};
pub use domain::types::{DownloadId, DownloadRequest, Quantization, ShardInfo};
pub use executor::{EventCallback, ExecutionResult, PythonDownloadExecutor};
pub use huggingface::{FileResolution, QuantizationFileResolver, resolve_quantization_files};
pub use progress::{ProgressContext, ProgressThrottle, build_queue_snapshot};
pub use queue::{DownloadQueue, FailedDownload, QueueSnapshot, QueuedDownload, ShardGroupId};
pub use service::{DownloadManager, DownloadManagerConfig};
