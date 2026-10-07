//! Download events - discriminated union for all download state changes.

use super::completion::QueueRunSummary;
use super::queue::{DownloadOutcome, FinishedDownload, QueueSnapshot};
use serde::{Deserialize, Serialize};

/// Single discriminated union for all download events.
///
/// The queue itself travels as [`DownloadEvent::QueueSnapshot`]: the same
/// [`QueueSnapshot`] the REST route serves, with every row's bytes, speed and
/// text. The other four say that something ended, for a notice to the user
/// and a refresh of the library; they carry no state a snapshot lacks. An
/// ending's `text` is its finished entry's, ready to print; the outcome
/// itself, with its message or error, is on that entry and not repeated here.
/// TypeScript reads this type through its generated binding; there is no
/// mirror to keep in step.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DownloadEvent {
    /// The whole queue. Sent when it changes, and four times a second while
    /// a download is transferring. Boxed because it is far the largest
    /// variant; the wire shape is the snapshot's own.
    QueueSnapshot(Box<QueueSnapshot>),

    /// Download completed successfully.
    DownloadCompleted {
        /// Canonical ID of the download.
        id: String,
        /// How it ended, in words: its finished entry's text.
        text: String,
    },

    /// Download failed with an error.
    DownloadFailed {
        /// Canonical ID of the download.
        id: String,
        /// How it ended, in words: its finished entry's text.
        text: String,
    },

    /// Download was cancelled by the user.
    DownloadCancelled {
        /// Canonical ID of the download.
        id: String,
        /// How it ended, in words: its finished entry's text.
        text: String,
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
    /// The event that carries `snapshot`.
    #[must_use]
    pub fn queue_snapshot(snapshot: QueueSnapshot) -> Self {
        Self::QueueSnapshot(Box::new(snapshot))
    }

    /// The event that says a download ended as `ended` records.
    #[must_use]
    pub fn ended(ended: &FinishedDownload) -> Self {
        let (id, text) = (ended.id.clone(), ended.text.clone());
        match &ended.outcome {
            DownloadOutcome::Completed { .. } => Self::DownloadCompleted { id, text },
            DownloadOutcome::Failed { .. } => Self::DownloadFailed { id, text },
            DownloadOutcome::Cancelled => Self::DownloadCancelled { id, text },
        }
    }

    /// Create a queue run complete event.
    pub const fn queue_run_complete(summary: QueueRunSummary) -> Self {
        Self::QueueRunComplete { summary }
    }

    /// Get the download ID from any event type.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::QueueSnapshot(_) | Self::QueueRunComplete { .. } => None,
            Self::DownloadCompleted { id, .. }
            | Self::DownloadFailed { id, .. }
            | Self::DownloadCancelled { id, .. } => Some(id),
        }
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
