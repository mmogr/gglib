#![doc = include_str!("README.md")]
pub mod completion;
pub mod errors;
pub mod events;
pub mod queue;
pub mod types;

// Re-export commonly used types
pub use completion::{
    AttemptCounts, CompletionDetail, CompletionKey, CompletionKind, QueueRunSummary,
};
pub use errors::{DownloadError, DownloadResult};
pub use events::{DownloadEvent, DownloadStatus, DownloadSummary};
pub use queue::{FailedDownload, QueueSnapshot, QueuedDownload};
pub use types::{DownloadId, Quantization, ShardInfo};
