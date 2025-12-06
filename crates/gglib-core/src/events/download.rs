//! Download progress and completion events.

use super::AppEvent;

impl AppEvent {
    /// Create a download started event.
    pub fn download_started(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self::DownloadStarted {
            id: id.into(),
            display_name: display_name.into(),
        }
    }

    /// Create a download progress event.
    pub fn download_progress(
        id: impl Into<String>,
        downloaded: u64,
        total: u64,
        speed_bps: f64,
        eta_seconds: f64,
        percentage: f64,
    ) -> Self {
        Self::DownloadProgress {
            id: id.into(),
            downloaded,
            total,
            speed_bps,
            eta_seconds,
            percentage,
        }
    }

    /// Create a download completed event.
    pub fn download_completed(id: impl Into<String>, message: Option<String>) -> Self {
        Self::DownloadCompleted {
            id: id.into(),
            message,
        }
    }

    /// Create a download failed event.
    pub fn download_failed(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self::DownloadFailed {
            id: id.into(),
            error: error.into(),
        }
    }

    /// Create a download cancelled event.
    pub fn download_cancelled(id: impl Into<String>) -> Self {
        Self::DownloadCancelled { id: id.into() }
    }
}
