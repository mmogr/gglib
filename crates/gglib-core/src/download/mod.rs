//! Download domain types, events, errors, and traits.
//!
//! This module contains pure data types and trait definitions for the download
//! system. No I/O, networking, or runtime dependencies allowed.
//!
//! # Structure
//!
//! - `types` - Core identifiers and data structures (DownloadId, Quantization, ShardInfo)
//! - `events` - Download events and status types (DownloadEvent, DownloadStatus)
//! - `errors` - Error types for download operations
//! - `queue` - Queue snapshot DTOs (QueueSnapshot, QueuedDownload, FailedDownload)

pub mod errors;
pub mod events;
pub mod queue;
pub mod types;

// Re-export commonly used types
pub use errors::{DownloadError, DownloadResult};
pub use events::{DownloadEvent, DownloadStatus, DownloadSummary};
pub use queue::{FailedDownload, QueueSnapshot, QueuedDownload};
pub use types::{DownloadId, Quantization, ShardInfo};
