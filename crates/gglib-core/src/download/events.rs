//! Download events - discriminated union for all download state changes.

use super::completion::QueueRunSummary;
use super::types::ShardInfo;
use serde::{Deserialize, Serialize};

/// A summary of a download in the queue (for snapshots and API responses).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadSummary {
    /// Canonical ID string (`model_id:quantization` or just `model_id`).
    pub id: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Current status of this download.
    pub status: DownloadStatus,
    /// Position in queue (1 = currently downloading, 2+ = waiting).
    pub position: u32,
    /// Error message if status is Failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Group ID for sharded downloads (all shards share the same `group_id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// Shard information if this is part of a sharded model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shard_info: Option<ShardInfo>,
}

/// Status of a download.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    /// Waiting in the queue.
    Queued,
    /// Currently being downloaded.
    Downloading,
    /// Completed successfully.
    Completed,
    /// Failed with an error.
    Failed,
    /// Cancelled by user.
    Cancelled,
}

impl DownloadStatus {
    /// Convert to string representation for database storage.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Downloading => "downloading",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parse from string representation.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "downloading" => Self::Downloading,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            // "queued" or unknown values default to Queued
            _ => Self::Queued,
        }
    }
}

/// Single discriminated union for all download events.
///
/// The frontend handles this as a TypeScript discriminated union:
///
/// ```typescript
/// type DownloadEvent =
///   | { type: "queue_snapshot"; items: DownloadSummary[]; max_size: number }
///   | { type: "download_started"; id: string }
///   | { type: "download_progress"; id: string; downloaded: number; total: number; ... }
///   | { type: "shard_progress"; id: string; shard_index: number; ... }
///   | { type: "download_completed"; id: string }
///   | { type: "download_failed"; id: string; error: string }
///   | { type: "download_cancelled"; id: string };
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DownloadEvent {
    /// Snapshot of the entire queue state.
    QueueSnapshot {
        /// All items currently in the queue.
        items: Vec<DownloadSummary>,
        /// Maximum queue capacity.
        max_size: u32,
    },

    /// A download has started.
    DownloadStarted {
        /// Canonical ID of the download.
        id: String,
    },

    /// Progress update for a non-sharded download.
    DownloadProgress {
        /// Canonical ID of the download.
        id: String,
        /// Bytes downloaded so far.
        downloaded: u64,
        /// Total bytes to download.
        total: u64,
        /// Current download speed in bytes per second.
        speed_bps: f64,
        /// Estimated time remaining in seconds.
        eta_seconds: f64,
        /// Progress percentage (0.0 - 100.0).
        percentage: f64,
    },

    /// Progress update for a sharded download.
    ShardProgress {
        /// Canonical ID of the download (group ID).
        id: String,
        /// Current shard index (0-based).
        shard_index: u32,
        /// Total number of shards.
        total_shards: u32,
        /// Filename of the current shard.
        shard_filename: String,
        /// Bytes downloaded for current shard.
        shard_downloaded: u64,
        /// Total bytes for current shard.
        shard_total: u64,
        /// Aggregate bytes downloaded across all shards.
        aggregate_downloaded: u64,
        /// Aggregate total bytes across all shards.
        aggregate_total: u64,
        /// Current download speed in bytes per second.
        speed_bps: f64,
        /// Estimated time remaining in seconds.
        eta_seconds: f64,
        /// Aggregate progress percentage (0.0 - 100.0).
        percentage: f64,
    },

    /// Download completed successfully.
    DownloadCompleted {
        /// Canonical ID of the download.
        id: String,
        /// Optional success message.
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },

    /// Download failed with an error.
    DownloadFailed {
        /// Canonical ID of the download.
        id: String,
        /// Error message describing what went wrong.
        error: String,
    },

    /// Download was cancelled by the user.
    DownloadCancelled {
        /// Canonical ID of the download.
        id: String,
    },

    /// Queue run completed (all downloads in the queue finished).
    ///
    /// Emitted when the download queue transitions from busy → idle,
    /// providing a complete summary of all artifacts that were processed
    /// during the run.
    QueueRunComplete {
        /// Complete summary of the queue run.
        summary: QueueRunSummary,
    },
}

impl DownloadEvent {
    /// Create a queue snapshot event.
    #[must_use]
    pub const fn queue_snapshot(items: Vec<DownloadSummary>, max_size: u32) -> Self {
        Self::QueueSnapshot { items, max_size }
    }

    /// Create a download started event.
    pub fn started(id: impl Into<String>) -> Self {
        Self::DownloadStarted { id: id.into() }
    }

    /// Create a non-sharded progress event.
    #[allow(clippy::cast_precision_loss)]
    pub fn progress(id: impl Into<String>, downloaded: u64, total: u64, speed_bps: f64) -> Self {
        let percentage = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let eta_seconds = if speed_bps > 0.0 && total > downloaded {
            (total - downloaded) as f64 / speed_bps
        } else {
            0.0
        };

        Self::DownloadProgress {
            id: id.into(),
            downloaded,
            total,
            speed_bps,
            eta_seconds,
            percentage,
        }
    }

    /// Create a sharded progress event.
    #[allow(clippy::too_many_arguments, clippy::cast_precision_loss)]
    pub fn shard_progress(
        id: impl Into<String>,
        shard_index: u32,
        total_shards: u32,
        shard_filename: impl Into<String>,
        shard_downloaded: u64,
        shard_total: u64,
        aggregate_downloaded: u64,
        aggregate_total: u64,
        speed_bps: f64,
    ) -> Self {
        let percentage = if aggregate_total > 0 {
            (aggregate_downloaded as f64 / aggregate_total as f64) * 100.0
        } else {
            0.0
        };

        let eta_seconds = if speed_bps > 0.0 && aggregate_total > aggregate_downloaded {
            (aggregate_total - aggregate_downloaded) as f64 / speed_bps
        } else {
            0.0
        };

        Self::ShardProgress {
            id: id.into(),
            shard_index,
            total_shards,
            shard_filename: shard_filename.into(),
            shard_downloaded,
            shard_total,
            aggregate_downloaded,
            aggregate_total,
            speed_bps,
            eta_seconds,
            percentage,
        }
    }

    /// Create a download completed event.
    pub fn completed(id: impl Into<String>, message: Option<impl Into<String>>) -> Self {
        Self::DownloadCompleted {
            id: id.into(),
            message: message.map(Into::into),
        }
    }

    /// Create a download failed event.
    pub fn failed(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self::DownloadFailed {
            id: id.into(),
            error: error.into(),
        }
    }

    /// Create a download cancelled event.
    pub fn cancelled(id: impl Into<String>) -> Self {
        Self::DownloadCancelled { id: id.into() }
    }

    /// Create a queue run complete event.
    pub const fn queue_run_complete(summary: QueueRunSummary) -> Self {
        Self::QueueRunComplete { summary }
    }

    /// Get the download ID from any event type.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::QueueSnapshot { .. } | Self::QueueRunComplete { .. } => None,
            Self::DownloadStarted { id }
            | Self::DownloadProgress { id, .. }
            | Self::ShardProgress { id, .. }
            | Self::DownloadCompleted { id, .. }
            | Self::DownloadFailed { id, .. }
            | Self::DownloadCancelled { id } => Some(id),
        }
    }

    /// Get the event name for wire protocols.
    ///
    /// This provides consistent event naming for Tauri and SSE transports.
    /// Note: Both `ShardProgress` and `DownloadProgress` use "download:progress"
    /// as the channel name; differentiation happens via the type discriminator.
    #[must_use]
    pub const fn event_name(&self) -> &'static str {
        match self {
            Self::QueueSnapshot { .. } => "download:queue_snapshot",
            Self::DownloadStarted { .. } => "download:started",
            Self::DownloadProgress { .. } | Self::ShardProgress { .. } => "download:progress",
            Self::DownloadCompleted { .. } => "download:completed",
            Self::DownloadFailed { .. } => "download:failed",
            Self::DownloadCancelled { .. } => "download:cancelled",
            Self::QueueRunComplete { .. } => "download:queue_run_complete",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_event_calculations() {
        let event = DownloadEvent::progress("id", 500, 1000, 100.0);
        match event {
            DownloadEvent::DownloadProgress {
                percentage,
                eta_seconds,
                ..
            } => {
                assert!((percentage - 50.0).abs() < 0.01);
                assert!((eta_seconds - 5.0).abs() < 0.01);
            }
            _ => panic!("Expected DownloadProgress"),
        }
    }

    #[test]
    fn test_event_id_extraction() {
        assert_eq!(DownloadEvent::started("test").id(), Some("test"));
        assert_eq!(DownloadEvent::cancelled("test").id(), Some("test"));
        assert!(DownloadEvent::queue_snapshot(vec![], 10).id().is_none());
    }
}
