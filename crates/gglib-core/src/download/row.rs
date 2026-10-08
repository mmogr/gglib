//! One download row, built from the facts of the download.
//!
//! [`row`] is the only place a [`DownloadRow`] is made, and
//! [`DownloadRowText::of`] the only place its words are, whoever holds the
//! facts: the download manager for its queue, or a command fetching files on
//! its own.

use serde::{Deserialize, Serialize};

use super::format::{format_duration, format_rate, format_size};
use super::types::DownloadId;

/// One download: every file of one model, as one line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct DownloadRow {
    /// Canonical ID (`model_id:quantization` or `model_id`). No two rows of a
    /// snapshot share one.
    pub id: String,
    /// Full model ID (e.g., "TheBloke/Llama-2-7B-GGUF").
    pub model_id: String,
    /// The quantization, when the download names one.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    /// What the download is doing.
    pub phase: DownloadPhase,
    /// 1 for the running download, then 2 and up in the order the waiting
    /// ones will run. With nothing running the first waiting one is 1.
    pub position: u32,
    /// Bytes on disk, over every file of the download.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub downloaded_bytes: u64,
    /// The size of every file together. Absent when any is unknown.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    /// `downloaded_bytes` of `total_bytes`, from 0 to 100. Absent with the
    /// total.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    /// Bytes per second off the network. Absent until there is enough to go
    /// on, and whenever the download is not transferring.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_bps: Option<f64>,
    /// Seconds until the last byte, absent like the speed.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<f64>,
    /// The row in words, ready to print.
    pub text: DownloadRowText,
}

/// What a download in the queue is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum DownloadPhase {
    /// Waiting for the download ahead of it.
    Queued,
    /// Fetching its files.
    Downloading,
    /// Every file is on disk, and the model's details are being gathered.
    Finalizing,
    /// The model is being added to the library.
    Registering,
}

/// A row's display text, made in one place so no renderer words it again.
///
/// A field that has nothing to say is an empty string.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct DownloadRowText {
    /// The download's name.
    pub title: String,
    /// Which file of the download this is about: `part 2/3`, `weights` or
    /// `projector` on the running row, `3 parts` on a waiting one. Absent
    /// for a download of one file.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// What it is doing: `Queued`, `Downloading`, `Finalizing…`,
    /// `Registering…`, or a note standing in for progress.
    pub status: String,
    /// `1.25 GiB / 27.92 GiB`; the bytes alone when the total is unknown,
    /// and the total alone on a waiting row.
    pub bytes: String,
    /// `42.3%`. Stays below `100.0%` until the last byte is in place.
    pub percent: String,
    /// `118.4 MB/s`, or a dash while it is not yet known.
    pub speed: String,
    /// `ETA 2m 40s`, or `ETA` and a dash while it is not yet known.
    pub eta: String,
}

/// The status of a download whose transfer is under way with nothing more
/// to say of it. A renderer that leaves the word out of a line of numbers
/// knows it by this.
pub const STATUS_DOWNLOADING: &str = "Downloading";

/// The name a download goes by wherever it is shown: its canonical ID,
/// `owner/repo:Q8_0`, or `owner/repo` when it names no quantization.
#[must_use]
pub fn download_title(id: &DownloadId) -> String {
    id.to_string()
}

/// Which file of its download a row is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilePlace {
    /// One shard of weights split over several: `part 2/3`.
    Part {
        /// The shard's number, counting from 1.
        number: u32,
        /// How many shards the weights are in. A projector is not one.
        of: u32,
    },
    /// The weights, in one file, of a download that also has a projector.
    Weights,
    /// The projector fetched with the weights.
    Projector,
    /// A waiting download whose weights are in this many shards: `3 parts`.
    Parts(u32),
}

impl FilePlace {
    fn label(self) -> String {
        match self {
            Self::Part { number, of } => format!("part {number}/{of}"),
            Self::Weights => "weights".to_string(),
            Self::Projector => "projector".to_string(),
            Self::Parts(count) => format!("{count} parts"),
        }
    }
}

/// What is known of a download, from which its row is built.
#[derive(Clone, Copy, Debug)]
pub struct RowFacts<'a> {
    /// The download.
    pub id: &'a DownloadId,
    /// What it is doing.
    pub phase: DownloadPhase,
    /// Its place among the downloads, counting from 1.
    pub position: u32,
    /// The file it is on, when it has more than one.
    pub place: Option<FilePlace>,
    /// Bytes on disk, over every file.
    pub bytes: u64,
    /// The size of every file together, when every one is known.
    pub total: Option<u64>,
    /// Bytes per second off the network, once measured.
    pub speed_bps: Option<f64>,
    /// Seconds remaining, once measured.
    pub eta_seconds: Option<f64>,
    /// A note standing in for progress while there is none.
    pub notice: Option<&'a str>,
}

impl<'a> RowFacts<'a> {
    /// The facts of a download that has not started.
    #[must_use]
    pub const fn waiting(
        id: &'a DownloadId,
        position: u32,
        place: Option<FilePlace>,
        total: Option<u64>,
    ) -> Self {
        Self {
            id,
            phase: DownloadPhase::Queued,
            position,
            place,
            bytes: 0,
            total,
            speed_bps: None,
            eta_seconds: None,
            notice: None,
        }
    }

    /// Whether bytes are moving, so a speed means something.
    fn is_transferring(&self) -> bool {
        self.phase == DownloadPhase::Downloading
    }

    /// The size of every file together. A total of 0 is no total.
    fn known_total(&self) -> Option<u64> {
        self.total.filter(|&total| total > 0)
    }
}

/// Build a download's row from its facts.
///
/// A speed and a time remaining are carried only while the download is
/// transferring: once its bytes are in, the last reading is history.
#[must_use]
pub fn row(facts: &RowFacts<'_>) -> DownloadRow {
    let total = facts.known_total();
    #[allow(clippy::cast_precision_loss)]
    let percent = total.map(|total| (facts.bytes as f64 / total as f64 * 100.0).clamp(0.0, 100.0));
    let transferring = facts.is_transferring();

    DownloadRow {
        id: facts.id.to_string(),
        model_id: facts.id.model_id().to_string(),
        quantization: facts.id.quantization().map(str::to_string),
        phase: facts.phase,
        position: facts.position,
        downloaded_bytes: facts.bytes,
        total_bytes: total,
        percent,
        speed_bps: facts.speed_bps.filter(|_| transferring),
        eta_seconds: facts.eta_seconds.filter(|_| transferring),
        text: DownloadRowText::of(facts),
    }
}

impl DownloadRowText {
    /// The words for a download with these facts.
    #[must_use]
    pub fn of(facts: &RowFacts<'_>) -> Self {
        let total = facts.known_total();
        let queued = facts.phase == DownloadPhase::Queued;
        let transferring = facts.is_transferring();

        let status = match facts.phase {
            DownloadPhase::Queued => "Queued",
            DownloadPhase::Downloading => facts.notice.unwrap_or(STATUS_DOWNLOADING),
            DownloadPhase::Finalizing => "Finalizing…",
            DownloadPhase::Registering => "Registering…",
        };
        let bytes = match (queued, total) {
            (true, Some(total)) => format_size(total),
            (true, None) => String::new(),
            (false, Some(total)) => {
                format!("{} / {}", format_size(facts.bytes), format_size(total))
            }
            (false, None) => format_size(facts.bytes),
        };
        let percent = total
            .filter(|_| !queued)
            .map_or_else(String::new, |total| percent_text(facts.bytes, total));

        Self {
            title: download_title(facts.id),
            file: facts.place.map(FilePlace::label),
            status: status.to_string(),
            bytes,
            percent,
            speed: if transferring {
                format_rate(facts.speed_bps)
            } else {
                String::new()
            },
            eta: if transferring {
                format!("ETA {}", format_duration(facts.eta_seconds))
            } else {
                String::new()
            },
        }
    }
}

/// `bytes` of `total` as a percentage to one decimal, rounded down.
///
/// Rounded down so that `100.0%` is read only when every byte is there: a
/// download a few kilobytes short of 28 GiB reads `99.9%`.
fn percent_text(bytes: u64, total: u64) -> String {
    let tenths = (u128::from(bytes.min(total)) * 1000) / u128::from(total);
    format!("{}.{}%", tenths / 10, tenths % 10)
}

#[cfg(test)]
#[path = "row_tests.rs"]
mod tests;
