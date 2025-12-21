//! Queue DTOs for API responses and snapshots.
//!
//! These types are "UI safe" - Clone + Debug + Serialize + Deserialize with no
//! infrastructure dependencies. They're used for transmitting queue state to
//! frontends via SSE, Tauri events, or CLI output.

use super::events::DownloadStatus;
use super::types::{Quantization, ShardInfo};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Snapshot of the entire download queue for API responses.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct QueueSnapshot {
    /// Items currently in the queue.
    pub items: Vec<QueuedDownload>,
    /// Maximum queue capacity.
    pub max_size: u32,
    /// Number of active downloads (currently downloading).
    pub active_count: u32,
    /// Number of pending downloads (queued, waiting).
    pub pending_count: u32,
    /// Recent failures (kept for UI display).
    pub recent_failures: Vec<FailedDownload>,
}

impl QueueSnapshot {
    /// Create a new empty snapshot.
    #[must_use]
    pub const fn new(max_size: u32) -> Self {
        Self {
            items: Vec::new(),
            max_size,
            active_count: 0,
            pending_count: 0,
            recent_failures: Vec::new(),
        }
    }

    /// Check if the queue is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Check if the queue is full.
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.items.len() >= self.max_size as usize
    }

    /// Get the total number of items.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.items.len()
    }

    /// Get an item by its ID.
    pub fn get(&self, id: &str) -> Option<&QueuedDownload> {
        self.items.iter().find(|item| item.id == id)
    }
}

/// A single download in the queue.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueuedDownload {
    /// Canonical ID (`model_id:quantization` or `model_id`).
    pub id: String,

    /// Full model ID (e.g., "TheBloke/Llama-2-7B-GGUF").
    pub model_id: String,

    /// Resolved quantization (if specified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<Quantization>,

    /// Human-readable display name.
    pub display_name: String,

    /// Current status.
    pub status: DownloadStatus,

    /// Position in queue (1-based; 1 = active, 2+ = waiting).
    pub position: u32,

    /// Bytes downloaded so far.
    pub downloaded_bytes: u64,

    /// Total bytes to download.
    pub total_bytes: u64,

    /// Download speed in bytes per second.
    pub speed_bps: f64,

    /// Estimated time remaining.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<f64>,

    /// Progress as percentage (0.0 - 100.0).
    pub progress_percent: f64,

    /// Timestamp when download was queued (Unix epoch seconds).
    pub queued_at: u64,

    /// Timestamp when download started (Unix epoch seconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,

    /// Group ID for sharded downloads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,

    /// Shard information if this is part of a sharded download.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shard_info: Option<ShardInfo>,
}

impl QueuedDownload {
    /// Create a new queued download in initial state.
    pub fn new(
        id: impl Into<String>,
        model_id: impl Into<String>,
        display_name: impl Into<String>,
        position: u32,
        queued_at: u64,
    ) -> Self {
        Self {
            id: id.into(),
            model_id: model_id.into(),
            quantization: None,
            display_name: display_name.into(),
            status: DownloadStatus::Queued,
            position,
            downloaded_bytes: 0,
            total_bytes: 0,
            speed_bps: 0.0,
            eta_seconds: None,
            progress_percent: 0.0,
            queued_at,
            started_at: None,
            group_id: None,
            shard_info: None,
        }
    }

    /// Set the quantization.
    #[must_use]
    pub const fn with_quantization(mut self, quant: Quantization) -> Self {
        self.quantization = Some(quant);
        self
    }

    /// Set the download status.
    #[must_use]
    pub const fn with_status(mut self, status: DownloadStatus) -> Self {
        self.status = status;
        self
    }

    /// Set shard information.
    #[must_use]
    pub fn with_shard_info(mut self, group_id: String, shard_info: ShardInfo) -> Self {
        self.group_id = Some(group_id);
        self.shard_info = Some(shard_info);
        self
    }

    /// Update progress from bytes downloaded.
    pub fn update_progress(&mut self, downloaded: u64, total: u64, speed_bps: f64) {
        self.downloaded_bytes = downloaded;
        self.total_bytes = total;
        self.speed_bps = speed_bps;

        self.progress_percent = if total > 0 {
            #[expect(
                clippy::cast_precision_loss,
                reason = "precision loss acceptable for progress percentage"
            )]
            let progress = (downloaded as f64 / total as f64) * 100.0;
            progress
        } else {
            0.0
        };

        self.eta_seconds = if speed_bps > 0.0 && total > downloaded {
            #[expect(
                clippy::cast_precision_loss,
                reason = "precision loss acceptable for ETA calculation"
            )]
            let eta = (total - downloaded) as f64 / speed_bps;
            Some(eta)
        } else {
            None
        };
    }

    /// Check if this download is currently active.
    pub fn is_active(&self) -> bool {
        self.status == DownloadStatus::Downloading
    }

    /// Check if this download is complete.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(
            self.status,
            DownloadStatus::Completed | DownloadStatus::Cancelled | DownloadStatus::Failed
        )
    }

    /// Get formatted speed string (e.g., "5.2 MB/s").
    pub fn speed_display(&self) -> String {
        format_bytes_per_second(self.speed_bps)
    }

    /// Get formatted ETA string (e.g., "2m 30s").
    #[must_use]
    pub fn eta_display(&self) -> Option<String> {
        self.eta_seconds.map(|secs| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "ETA seconds are always positive and within u64 range"
            )]
            let secs_u64 = secs as u64;
            format_duration(secs_u64)
        })
    }
}

/// A failed download kept for display purposes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FailedDownload {
    /// Canonical ID of the failed download.
    pub id: String,

    /// Display name.
    pub display_name: String,

    /// Error message.
    pub error: String,

    /// Timestamp when the failure occurred (Unix epoch seconds).
    pub failed_at: u64,

    /// Whether the failure is recoverable (can retry).
    pub recoverable: bool,

    /// Bytes downloaded before failure.
    pub downloaded_bytes: u64,
}

impl FailedDownload {
    /// Create a new failed download record.
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        error: impl Into<String>,
        failed_at: u64,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            error: error.into(),
            failed_at,
            recoverable: false,
            downloaded_bytes: 0,
        }
    }

    /// Mark as recoverable.
    #[must_use]
    pub const fn with_recoverable(mut self, recoverable: bool) -> Self {
        self.recoverable = recoverable;
        self
    }

    /// Set bytes downloaded before failure.
    #[must_use]
    pub const fn with_downloaded_bytes(mut self, bytes: u64) -> Self {
        self.downloaded_bytes = bytes;
        self
    }
}

/// Format bytes per second as human-readable string.
fn format_bytes_per_second(bps: f64) -> String {
    let (value, unit) = if bps >= 1_000_000_000.0 {
        (bps / 1_000_000_000.0, "GB/s")
    } else if bps >= 1_000_000.0 {
        (bps / 1_000_000.0, "MB/s")
    } else if bps >= 1_000.0 {
        (bps / 1_000.0, "KB/s")
    } else {
        return format!("{bps:.0} B/s");
    };
    format!("{value:.1} {unit}")
}

/// Format seconds as human-readable duration.
fn format_duration(secs: u64) -> String {
    let duration = Duration::from_secs(secs);
    let hours = duration.as_secs() / 3600;
    let minutes = (duration.as_secs() % 3600) / 60;
    let seconds = duration.as_secs() % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_snapshot_operations() {
        let mut snapshot = QueueSnapshot::new(10);
        assert!(snapshot.is_empty());
        assert!(!snapshot.is_full());

        snapshot
            .items
            .push(QueuedDownload::new("id1", "model", "Display", 1, 0));
        assert!(!snapshot.is_empty());
        assert_eq!(snapshot.len(), 1);
        assert!(snapshot.get("id1").is_some());
        assert!(snapshot.get("nonexistent").is_none());
    }

    #[test]
    fn test_queued_download_progress() {
        let mut download = QueuedDownload::new("id", "model", "Display", 1, 0);
        download.update_progress(500, 1000, 100.0);

        assert_eq!(download.downloaded_bytes, 500);
        assert!((download.progress_percent - 50.0).abs() < 0.01);
        assert!((download.eta_seconds.unwrap() - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_speed_display() {
        let mut download = QueuedDownload::new("id", "model", "Display", 1, 0);

        download.speed_bps = 5_000_000.0;
        assert_eq!(download.speed_display(), "5.0 MB/s");

        download.speed_bps = 1_500_000_000.0;
        assert_eq!(download.speed_display(), "1.5 GB/s");

        download.speed_bps = 500.0;
        assert_eq!(download.speed_display(), "500 B/s");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3661), "1h 1m");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let download = QueuedDownload::new("id", "model", "Display", 1, 1_234_567_890)
            .with_quantization(Quantization::Q4KM);

        let json = serde_json::to_string(&download).unwrap();
        let parsed: QueuedDownload = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "id");
        assert_eq!(parsed.quantization, Some(Quantization::Q4KM));
    }
}
