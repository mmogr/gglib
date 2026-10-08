//! The download queue as every surface is shown it.
//!
//! One [`QueueSnapshot`] is built by the download manager and served unchanged
//! on REST and on the event stream. A download is one [`DownloadRow`] however
//! many files it has, and the row carries its own display text, so the CLI and
//! the GUI print the same words for the same download.

use serde::{Deserialize, Serialize};

use super::row::{DownloadRow, download_title};
use super::types::DownloadId;

/// How many finished downloads a snapshot keeps.
pub const FINISHED_LIMIT: usize = 16;

/// The whole download queue at one moment.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct QueueSnapshot {
    /// Counts the snapshots this process has built. A later snapshot has a
    /// higher number, on REST and on the event stream alike. It starts again
    /// when the process does.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub revision: u64,
    /// The download that is running, if there is one. It stays here between
    /// two of its files.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<DownloadRow>,
    /// The downloads waiting behind it, in the order they will run.
    pub waiting: Vec<DownloadRow>,
    /// How the most recent downloads ended, oldest first. At most
    /// [`FINISHED_LIMIT`], and one entry per download: a download that ends
    /// again replaces its earlier entry.
    pub finished: Vec<FinishedDownload>,
    /// How many downloads may wait at once.
    pub max_size: u32,
    /// Whether that many are waiting, so another would be refused.
    pub full: bool,
}

impl QueueSnapshot {
    /// The running download and then the waiting ones.
    pub fn rows(&self) -> impl Iterator<Item = &DownloadRow> {
        self.active.iter().chain(&self.waiting)
    }

    /// Whether nothing is running and nothing is waiting.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.active.is_none() && self.waiting.is_empty()
    }
}

/// How a download ended.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct FinishedDownload {
    /// The download's canonical ID.
    pub id: String,
    /// The download's name, as its row had it.
    pub title: String,
    /// What became of it.
    pub outcome: DownloadOutcome,
    /// How it ended, in words, ready to print. Made here and nowhere else,
    /// so a toast, a terminal line and a command's error all read the same.
    pub text: String,
}

impl FinishedDownload {
    /// The entry for the download `id`, which ended with `outcome`.
    #[must_use]
    pub fn new(id: &DownloadId, outcome: DownloadOutcome) -> Self {
        let title = download_title(id);
        let text = match &outcome {
            DownloadOutcome::Completed { message: None } => format!("{title}: downloaded"),
            DownloadOutcome::Completed {
                message: Some(message),
            } => format!("{title}: {message}"),
            DownloadOutcome::Failed { error } => format!("{title}: download failed: {error}"),
            DownloadOutcome::Cancelled => format!("{title}: download cancelled"),
        };
        Self {
            id: id.to_string(),
            title,
            outcome,
            text,
        }
    }
}

/// What became of a download.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DownloadOutcome {
    /// Every file is on disk and the model is in the library.
    Completed {
        /// A note on the result, e.g. that a projector was not linked.
        #[cfg_attr(feature = "ts-bindings", ts(optional))]
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// A file could not be fetched, or the model could not be added to the
    /// library.
    Failed {
        /// Why.
        error: String,
    },
    /// The user stopped it.
    Cancelled,
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
