#![doc = include_str!("README.md")]
pub(crate) mod completion;
pub(crate) mod errors;
pub(crate) mod events;
pub(crate) mod file_role;
pub(crate) mod format;
pub(crate) mod projector_choice;
pub mod queue;
pub(crate) mod rate;
pub(crate) mod shard_info;
pub(crate) mod throttle;
pub(crate) mod types;

// Re-export commonly used types
pub use completion::{
    AttemptCounts, CompletionDetail, CompletionKey, CompletionKind, QueueRunSummary,
};
pub use errors::DownloadError;
pub use events::{DownloadEvent, DownloadStatus, DownloadSummary};
pub use file_role::GgufFileRole;
pub use format::{format_duration, format_rate};
pub use projector_choice::choose_projector;
pub use queue::{FailedDownload, QueueSnapshot, QueuedDownload};
pub use rate::RateEstimator;
pub use shard_info::ShardInfo;
pub use throttle::ProgressThrottle;
pub use types::{DownloadId, Quantization};
