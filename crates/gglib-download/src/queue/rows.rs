//! The queue counted in downloads.
//!
//! `pending` holds one item per file, and every file of a download carries
//! the download's id. Rows, positions and capacity are all counted in
//! downloads, by the rules here.
//!
//! One download may be running. While one of its files is being fetched it is
//! the active job; between two of its files nothing is active and its next
//! file is the head of `pending`. Either way the caller names it, and its
//! pending files are not waiting: they stay at the head of `pending`, they
//! are not a row of their own, and they do not count toward capacity.

use gglib_core::download::{DownloadId, DownloadStatus, QueuedDownload};

use super::{DownloadQueue, QueuedItem, ShardGroupId};

/// The position of the first waiting download: 2 behind a running download,
/// which holds position 1, and 1 otherwise.
pub(super) const fn first_waiting_position(has_running: bool) -> u32 {
    if has_running { 2 } else { 1 }
}

impl DownloadQueue {
    /// The download and group of the file at the head of `pending`.
    ///
    /// This is the running download when nothing is active and the tracker
    /// has that group open: its earlier files are in, and this one is next.
    pub(crate) fn head_group(&self) -> Option<(&DownloadId, &ShardGroupId)> {
        let head = self.pending.front()?;
        Some((&head.id, head.group_id.as_ref()?))
    }

    /// The row of the `running` download while it is between two of its
    /// files: downloading, at position 1, with the place of its next file.
    ///
    /// The bytes are those in before that file, when the size of every file
    /// of the group is known, so the bar stays where the last file left it.
    pub(crate) fn between_files_row(&self, running: &DownloadId) -> Option<QueuedDownload> {
        let next = self.pending.front().filter(|item| &item.id == running)?;
        let mut row = next.to_dto(1, DownloadStatus::Downloading);
        if let Some((downloaded, total)) = next.shard_info.as_ref().and_then(|s| s.aggregate(0)) {
            row.update_progress(downloaded, total, None, None);
        }
        Some(row)
    }

    /// The first pending file of each waiting download, in the order they
    /// will run.
    pub(super) fn waiting(&self, running: Option<&DownloadId>) -> Vec<&QueuedItem> {
        self.waiting_but(|item| Some(&item.id) == running)
    }

    /// [`Self::waiting`], with the running download named by a test of its
    /// files.
    pub(super) fn waiting_but(&self, is_running: impl Fn(&QueuedItem) -> bool) -> Vec<&QueuedItem> {
        let mut first_files: Vec<&QueuedItem> = Vec::new();
        for item in &self.pending {
            if !is_running(item) && first_files.iter().all(|first| first.id != item.id) {
                first_files.push(item);
            }
        }
        first_files
    }

    /// Where in `pending` the waiting download in `slot` begins, counting
    /// from 0. A slot past the last waiting download is the end of the queue.
    pub(super) fn waiting_index(&self, running: Option<&DownloadId>, slot: usize) -> usize {
        self.waiting(running)
            .get(slot)
            .and_then(|first| self.pending.iter().position(|item| item.id == first.id))
            .unwrap_or(self.pending.len())
    }
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
