//! Queue DTOs for API responses and snapshots.
//!
//! These types are "UI safe" - Clone + Debug + Serialize + Deserialize with no
//! infrastructure dependencies. They're used for transmitting queue state to
//! frontends via SSE, Tauri events, or CLI output.

use super::events::DownloadStatus;
use super::format::{format_duration, format_rate};
use super::types::{Quantization, ShardInfo};
use serde::{Deserialize, Serialize};

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

    /// Download speed in bytes per second; absent until the estimator warms up.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_bps: Option<f64>,

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
            speed_bps: None,
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
    ///
    /// `speed_bps` and `eta_seconds` come from the download manager's
    /// `RateEstimator`; this type does not derive a rate or an ETA of its own.
    pub fn update_progress(
        &mut self,
        downloaded: u64,
        total: u64,
        speed_bps: Option<f64>,
        eta_seconds: Option<f64>,
    ) {
        self.downloaded_bytes = downloaded;
        self.total_bytes = total;
        self.speed_bps = speed_bps;
        self.eta_seconds = eta_seconds;

        self.progress_percent = if total > 0 {
            #[expect(
                clippy::cast_precision_loss,
                reason = "precision loss acceptable for progress percentage"
            )]
            let progress = (downloaded as f64 / total as f64) * 100.0;
            progress.clamp(0.0, 100.0)
        } else {
            0.0
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

    /// Get formatted speed string (e.g., `5.2 MB/s`, or a placeholder).
    #[must_use]
    pub fn speed_display(&self) -> String {
        format_rate(self.speed_bps)
    }

    /// Get formatted ETA string (e.g., `2m 30s`, or a placeholder).
    #[must_use]
    pub fn eta_display(&self) -> String {
        format_duration(self.eta_seconds)
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
        download.update_progress(500, 1000, Some(100.0), Some(5.0));

        assert_eq!(download.downloaded_bytes, 500);
        assert!((download.progress_percent - 50.0).abs() < 0.01);
        // Stored, not derived — the manager's estimator owns this number.
        assert!((download.eta_seconds.unwrap() - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_speed_display() {
        // Unit selection and thresholds are covered in `super::format`; this
        // only checks that the DTO delegates there rather than formatting
        // its own way.
        let mut download = QueuedDownload::new("id", "model", "Display", 1, 0);

        download.speed_bps = Some(5_000_000.0);
        assert_eq!(download.speed_display(), "5.0 MB/s");

        download.speed_bps = Some(1_500_000_000.0);
        assert_eq!(download.speed_display(), "1.50 GB/s");
    }

    #[test]
    fn display_helpers_show_a_placeholder_when_unknown() {
        let download = QueuedDownload::new("id", "model", "Display", 1, 0);
        assert_eq!(download.speed_display(), crate::download::format::UNKNOWN);
        assert_eq!(download.eta_display(), crate::download::format::UNKNOWN);
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

    /// Comprehensive test: verify `is_active()` and `is_complete()` for all 7 `DownloadStatus` variants.
    #[test]
    fn test_status_classification_all_variants() {
        let base = QueuedDownload::new("test-id", "test-model", "test-display", 1, 0);

        // Queued: not active, not complete
        let queued = base.clone().with_status(DownloadStatus::Queued);
        assert!(!queued.is_active(), "Queued should not be active");
        assert!(!queued.is_complete(), "Queued should not be complete");

        // Downloading: active, not complete
        let downloading = base.clone().with_status(DownloadStatus::Downloading);
        assert!(downloading.is_active(), "Downloading should be active");
        assert!(
            !downloading.is_complete(),
            "Downloading should not be complete"
        );

        // Finalizing: not active, not complete
        let finalizing = base.clone().with_status(DownloadStatus::Finalizing);
        assert!(!finalizing.is_active(), "Finalizing should not be active");
        assert!(
            !finalizing.is_complete(),
            "Finalizing should not be complete"
        );

        // Registering: not active, not complete
        let registering = base.clone().with_status(DownloadStatus::Registering);
        assert!(!registering.is_active(), "Registering should not be active");
        assert!(
            !registering.is_complete(),
            "Registering should not be complete"
        );

        // Completed: not active, complete
        let completed = base.clone().with_status(DownloadStatus::Completed);
        assert!(!completed.is_active(), "Completed should not be active");
        assert!(completed.is_complete(), "Completed should be complete");

        // Failed: not active, complete
        let failed = base.clone().with_status(DownloadStatus::Failed);
        assert!(!failed.is_active(), "Failed should not be active");
        assert!(failed.is_complete(), "Failed should be complete");

        // Cancelled: not active, complete
        let cancelled = base.with_status(DownloadStatus::Cancelled);
        assert!(!cancelled.is_active(), "Cancelled should not be active");
        assert!(cancelled.is_complete(), "Cancelled should be complete");
    }

    /// Test `update_progress` when downloaded bytes exceed total bytes.
    /// This documents the current behavior: `progress_percent` can exceed 100%, and `eta_seconds` becomes None.
    #[test]
    fn test_update_progress_downloaded_exceeds_total() {
        let mut download = QueuedDownload::new("test-id", "test-model", "test-display", 1, 0);

        // Call update_progress with downloaded > total
        download.update_progress(1500, 1000, Some(100.0), None);

        // Progress percent exceeds 100% (no clamping)
        // Clamped: a bar cannot be more than full, and an overshoot here means
        // the byte counter is double-counting, not that 150% of the file exists.
        assert!(
            (download.progress_percent - 100.0).abs() < 0.01,
            "Progress should clamp to 100.0%"
        );

        assert!(
            download.eta_seconds.is_none(),
            "ETA should be None when downloaded >= total"
        );

        assert_eq!(download.downloaded_bytes, 1500);
        assert!((download.speed_bps.unwrap() - 100.0).abs() < 0.01);
    }

    /// Test `update_progress` with zero total bytes (division-by-zero guard).
    #[test]
    #[allow(clippy::float_cmp)]
    fn test_update_progress_zero_total_bytes() {
        let mut download = QueuedDownload::new("test-id", "test-model", "test-display", 1, 0);

        // Call update_progress with total = 0
        download.update_progress(500, 0, Some(100.0), None);

        // Progress percent is 0.0 (the `if total > 0` guard prevents division by zero)
        assert_eq!(
            download.progress_percent, 0.0,
            "Progress should be 0.0 when total is 0"
        );

        // ETA is None because `total > downloaded` is false when total is 0
        assert!(
            download.eta_seconds.is_none(),
            "ETA should be None when total is 0"
        );

        // Downloaded bytes and speed are still updated
        assert_eq!(download.downloaded_bytes, 500);
        assert!((download.speed_bps.unwrap() - 100.0).abs() < 0.01);
    }

    /// Test `update_progress` with zero speed (division-by-zero guard for ETA).
    #[test]
    #[allow(clippy::float_cmp)]
    fn test_update_progress_zero_speed() {
        let mut download = QueuedDownload::new("test-id", "test-model", "test-display", 1, 0);

        // Call update_progress with speed = 0.0
        download.update_progress(500, 1000, Some(0.0), None);

        // Progress percent is still calculated correctly (50%)
        assert_eq!(download.progress_percent, 50.0, "Progress should be 50.0%");

        // A zero rate means "stalled". The estimator reports no ETA for it,
        // and the DTO stores that absence rather than inventing a 0.
        assert!(
            download.eta_seconds.is_none(),
            "ETA should be None when speed is 0"
        );

        assert_eq!(download.downloaded_bytes, 500);
        assert_eq!(download.speed_bps, Some(0.0));
    }

    /// Test `update_progress` when download is complete (downloaded == total).
    #[test]
    #[allow(clippy::float_cmp)]
    fn test_update_progress_complete_download() {
        let mut download = QueuedDownload::new("test-id", "test-model", "test-display", 1, 0);

        // Call update_progress with downloaded equal to total
        download.update_progress(1000, 1000, Some(100.0), None);

        // Progress percent should be 100%
        assert_eq!(
            download.progress_percent, 100.0,
            "Progress should be 100.0%"
        );

        // ETA is None because `total > downloaded` guard is false when equal
        assert!(
            download.eta_seconds.is_none(),
            "ETA should be None when download is complete"
        );

        // Downloaded bytes and speed are updated normally
        assert_eq!(download.downloaded_bytes, 1000);
        assert!((download.speed_bps.unwrap() - 100.0).abs() < 0.01);
    }

    /// Test `update_progress` with large u64 values — verifies no overflow/panic and results are in the right ballpark despite f64 precision loss.
    #[test]
    fn test_update_progress_large_u64_values() {
        let mut download = QueuedDownload::new("test-id", "test-model", "test-display", 1, 0);

        // 50 TB downloaded out of 100 TB total, at 1 GB/s
        let downloaded: u64 = 50_000_000_000_000;
        let total: u64 = 100_000_000_000_000;
        let speed_bps: f64 = 1_000_000_000.0; // 1 GB/s

        download.update_progress(downloaded, total, Some(speed_bps), Some(50_000.0));

        // Progress should be approximately 50% (within tolerance for f64 precision loss)
        assert!(
            (download.progress_percent - 50.0).abs() < 0.1,
            "Progress should be ~50%, got {}",
            download.progress_percent
        );

        // ETA is stored verbatim from the estimator.
        let eta = download.eta_seconds.expect("ETA should be Some");
        assert!(
            (eta - 50_000.0).abs() < 1.0,
            "ETA should be ~50,000 seconds, got {eta}"
        );

        // Verify downloaded_bytes and speed were updated
        assert_eq!(download.downloaded_bytes, downloaded);
        assert!((download.speed_bps.unwrap() - speed_bps).abs() < 0.01);
    }

    /// Test `FailedDownload` builder pattern — defaults and chained setters.
    #[test]
    fn test_failed_download_builders() {
        // Create with new() and verify defaults
        let failed = FailedDownload::new("id", "Display", "network error", 1_234_567_890);

        assert_eq!(failed.id, "id");
        assert_eq!(failed.display_name, "Display");
        assert_eq!(failed.error, "network error");
        assert_eq!(failed.failed_at, 1_234_567_890);
        // Defaults
        assert!(!failed.recoverable, "recoverable should default to false");
        assert_eq!(
            failed.downloaded_bytes, 0,
            "downloaded_bytes should default to 0"
        );

        // Chain builders and verify values are set
        let failed2 = FailedDownload::new("id2", "Display2", "timeout", 0)
            .with_recoverable(true)
            .with_downloaded_bytes(500_000);

        assert!(
            failed2.recoverable,
            "recoverable should be true after with_recoverable(true)"
        );
        assert_eq!(
            failed2.downloaded_bytes, 500_000,
            "downloaded_bytes should be 500_000"
        );
    }

    /// Test `QueueSnapshot::default()` produces the same state as `QueueSnapshot::new(0)`.
    #[test]
    fn test_queue_snapshot_default() {
        let default_snapshot = QueueSnapshot::default();
        let zero_snapshot = QueueSnapshot::new(0);

        // max_size should be 0 for both
        assert_eq!(default_snapshot.max_size, 0);
        assert_eq!(zero_snapshot.max_size, 0);
        assert_eq!(default_snapshot.max_size, zero_snapshot.max_size);

        // items should be empty
        assert!(default_snapshot.items.is_empty());
        assert_eq!(default_snapshot.items.len(), zero_snapshot.items.len());

        // counts should be zero
        assert_eq!(default_snapshot.active_count, 0);
        assert_eq!(default_snapshot.pending_count, 0);

        // recent_failures should be empty
        assert!(default_snapshot.recent_failures.is_empty());
        assert_eq!(
            default_snapshot.recent_failures.len(),
            zero_snapshot.recent_failures.len()
        );
    }
}
