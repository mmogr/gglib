//! Progress types.

/// A delta in download progress.
#[derive(Debug, Clone)]
pub struct ProgressDelta {
    /// Bytes downloaded since last update.
    pub bytes_delta: u64,
    /// Current total bytes downloaded.
    pub bytes_downloaded: u64,
    /// Total bytes to download.
    pub total_bytes: u64,
}

/// A snapshot of current progress for a download.
#[derive(Debug, Clone)]
pub struct ProgressSnapshot {
    /// Bytes downloaded so far.
    pub downloaded_bytes: u64,
    /// Total bytes to download.
    pub total_bytes: u64,
    /// Current download speed in bytes per second.
    pub speed_bps: f64,
    /// Estimated time remaining in seconds.
    pub eta_seconds: Option<f64>,
    /// Progress as percentage (0.0 - 100.0).
    pub progress_percent: f64,
}

impl ProgressSnapshot {
    /// Create a new progress snapshot.
    pub fn new(downloaded: u64, total: u64, speed_bps: f64) -> Self {
        let progress_percent = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let eta_seconds = if speed_bps > 0.0 && total > downloaded {
            Some((total - downloaded) as f64 / speed_bps)
        } else {
            None
        };

        Self {
            downloaded_bytes: downloaded,
            total_bytes: total,
            speed_bps,
            eta_seconds,
            progress_percent,
        }
    }
}
