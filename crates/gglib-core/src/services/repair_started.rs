//! What a repair answers with, and what is said of its files when the
//! download does not bring them back.
//!
//! Beside `model_verification_remote.rs`, which holds the repair itself and
//! is at the size budget (`scripts/check_rust_complexity.sh`).

use serde::{Deserialize, Serialize};

use crate::download::DownloadId;

/// A repair under way: its unhealthy files are off the disk, and the
/// download that fetches them again is queued and started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RepairStarted {
    /// The download's canonical ID: the row to watch in the queue.
    pub id: String,
    /// The files the download is to bring back, as the model's rows name
    /// them. None of them is on disk when the repair answers.
    pub files: Vec<String>,
}

/// What to say of `files`, missing from a model's folder since a repair, when
/// `download` did not bring them back: their names, and the command that
/// fetches them.
#[must_use]
pub fn missing_after_repair(download: &DownloadId, files: &[String]) -> String {
    let quantization = download
        .quantization()
        .map_or_else(String::new, |q| format!(" --quantization {q}"));
    format!(
        "Missing from the model's folder: {}. Run `gglib model download {}{quantization}` to \
         fetch what is missing.",
        files.join(", "),
        download.model_id()
    )
}
