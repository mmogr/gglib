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
    pub fn download_progress(
        id: impl Into<String>,
        downloaded: u64,
        total: u64,
        speed_bps: f64,
        eta_seconds: f64,
        percentage: f64,
    ) -> Self {
        let _ = (eta_seconds, percentage); // DownloadEvent::progress calculates these
        Self::Download {
            event: DownloadEvent::progress(id, downloaded, total, speed_bps),
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
