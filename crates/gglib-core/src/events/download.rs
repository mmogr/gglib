//! Download progress and completion events.

use super::AppEvent;
use crate::download::DownloadEvent;

impl AppEvent {
    /// Create a download started event.
    pub fn download_started(id: impl Into<String>, _display_name: impl Into<String>) -> Self {
        Self::Download {
            event: DownloadEvent::started(id),
        }
    }

    /// Create a download progress event.
    ///
    /// `speed_bps` and `eta_seconds` come from the download manager's
    /// `RateEstimator` and are `None` until it has warmed up; the percentage is
    /// derived from the byte counts.
    pub fn download_progress(
        id: impl Into<String>,
        downloaded: u64,
        total: u64,
        speed_bps: Option<f64>,
        eta_seconds: Option<f64>,
    ) -> Self {
        Self::Download {
            event: DownloadEvent::progress(id, downloaded, total, speed_bps, eta_seconds),
        }
    }

    /// Create a download completed event.
    pub fn download_completed(id: impl Into<String>, message: Option<String>) -> Self {
        Self::Download {
            event: DownloadEvent::completed(id, message),
        }
    }

    /// Create a download failed event.
    pub fn download_failed(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self::Download {
            event: DownloadEvent::failed(id, error),
        }
    }

    /// Create a download cancelled event.
    pub fn download_cancelled(id: impl Into<String>) -> Self {
        Self::Download {
            event: DownloadEvent::cancelled(id),
        }
    }
}
