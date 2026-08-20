#![doc = include_str!("README.md")]
pub(crate) mod completion;
pub(crate) mod errors;
pub(crate) mod events;
pub(crate) mod format;
pub mod queue;
pub(crate) mod rate;
pub(crate) mod types;

// Re-export commonly used types
pub use completion::{
    AttemptCounts, CompletionDetail, CompletionKey, CompletionKind, QueueRunSummary,
};
pub use errors::DownloadError;
pub use events::{DownloadEvent, DownloadStatus, DownloadSummary};
pub use format::{format_duration, format_rate};
pub use queue::{FailedDownload, QueueSnapshot, QueuedDownload};
pub use rate::RateEstimator;
pub use types::{DownloadId, Quantization, ShardInfo};
