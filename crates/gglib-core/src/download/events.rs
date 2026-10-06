//! Download events - discriminated union for all download state changes.

use super::completion::QueueRunSummary;
use super::queue::{DownloadOutcome, QueueSnapshot};
use serde::{Deserialize, Serialize};

/// Single discriminated union for all download events.
///
/// The queue itself travels as [`DownloadEvent::QueueSnapshot`]: the same
/// [`QueueSnapshot`] the REST route serves, with every row's bytes, speed and
/// text. The other four say that something ended, for a notice to the user
/// and a refresh of the library; they carry no state a snapshot lacks.
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
        /// Optional success message.
        #[cfg_attr(feature = "ts-bindings", ts(optional))]
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
    /// The event that carries `snapshot`.
    #[must_use]
    pub fn queue_snapshot(snapshot: QueueSnapshot) -> Self {
        Self::QueueSnapshot(Box::new(snapshot))
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

    /// The event that says a download ended with `outcome`.
    #[must_use]
    pub fn ended(id: &str, outcome: &DownloadOutcome) -> Self {
        match outcome {
            DownloadOutcome::Completed { message } => Self::completed(id, message.as_deref()),
            DownloadOutcome::Failed { error } => Self::failed(id, error),
            DownloadOutcome::Cancelled => Self::cancelled(id),
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
            | Self::DownloadCancelled { id } => Some(id),
        }
    }

    /// Colon-separated names — five, reached through `AppEvent`'s one download
    /// arm; `download_event_names_are_stable` pins them. **Not the wire
    /// format**: `AppEvent`'s `type` tag is `download`, and these retired
    /// Tauri-bus spellings are read by nothing.
    #[must_use]
    pub const fn event_name(&self) -> &'static str {
        match self {
            Self::QueueSnapshot(_) => "download:queue_snapshot",
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
    fn test_event_id_extraction() {
        assert_eq!(DownloadEvent::failed("test", "e").id(), Some("test"));
        assert_eq!(DownloadEvent::cancelled("test").id(), Some("test"));
        let snapshot = DownloadEvent::queue_snapshot(QueueSnapshot::default());
        assert!(snapshot.id().is_none());
    }

    /// The snapshot's own keys sit beside the event's `type`, so the event
    /// stream and the REST route carry one shape.
    #[test]
    fn a_snapshot_event_is_the_snapshot_with_a_type() {
        let snapshot = QueueSnapshot {
            revision: 4,
            max_size: 10,
            ..QueueSnapshot::default()
        };

        let event = serde_json::to_value(DownloadEvent::queue_snapshot(snapshot.clone()))
            .expect("serializes");
        let mut rest = serde_json::to_value(&snapshot).expect("serializes");
        rest["type"] = "queue_snapshot".into();

        assert_eq!(event, rest);
        let back: DownloadEvent = serde_json::from_value(event).expect("parses");
        assert!(matches!(back, DownloadEvent::QueueSnapshot(s) if *s == snapshot));
    }

    #[test]
    fn an_outcome_has_its_terminal_event() {
        let completed = DownloadOutcome::Completed {
            message: Some("ok".to_string()),
        };
        let failed = DownloadOutcome::Failed {
            error: "no".to_string(),
        };

        assert!(matches!(
            DownloadEvent::ended("id", &completed),
            DownloadEvent::DownloadCompleted { id, message } if id == "id" && message.as_deref() == Some("ok")
        ));
        assert!(matches!(
            DownloadEvent::ended("id", &failed),
            DownloadEvent::DownloadFailed { error, .. } if error == "no"
        ));
        assert!(matches!(
            DownloadEvent::ended("id", &DownloadOutcome::Cancelled),
            DownloadEvent::DownloadCancelled { .. }
        ));
    }
}
