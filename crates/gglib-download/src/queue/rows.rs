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

use gglib_core::download::{DownloadId, DownloadPhase, DownloadRow, RowFacts, ShardInfo, row};

use super::{DownloadQueue, QueuedItem, ShardGroupId, usize_to_u32_saturating};

/// The running download, as the manager names it to the queue.
#[derive(Clone, Debug)]
pub(crate) struct Running {
    /// Its id.
    pub id: DownloadId,
    /// What it is doing.
    pub phase: DownloadPhase,
    /// The file being fetched, or the one that is next while it is between
    /// two.
    pub file: Option<ShardInfo>,
}

/// What the running download's meter reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Reading {
    /// Bytes on disk, over every file of the download.
    pub bytes: u64,
    /// The size of every file together, when known.
    pub total: Option<u64>,
    /// Bytes per second off the network, once measured.
    pub speed_bps: Option<f64>,
    /// Seconds remaining, once measured.
    pub eta_seconds: Option<f64>,
    /// A note standing in for progress while there is none.
    pub notice: Option<String>,
}

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

    /// The next file of the `running` download while it is between two of
    /// its files.
    pub(crate) fn next_file_of(&self, running: &DownloadId) -> Option<&ShardInfo> {
        let next = self.pending.front().filter(|item| &item.id == running)?;
        next.shard_info.as_ref()
    }

    /// The queue's rows: the running download's, and one for each waiting
    /// download in the order they will run.
    ///
    /// The running row is at position 1 with what its meter reads, whether a
    /// file of it is being fetched or it is between two. Without a reading it
    /// has moved nothing yet, and its size is its group's.
    pub(crate) fn download_rows(
        &self,
        running: Option<&Running>,
        reading: Option<&Reading>,
    ) -> (Option<DownloadRow>, Vec<DownloadRow>) {
        let active = running.map(|running| {
            let file = running.file.as_ref();
            row(&RowFacts {
                id: &running.id,
                phase: running.phase,
                position: 1,
                place: file.and_then(ShardInfo::place),
                bytes: reading.map_or(0, |reading| reading.bytes),
                total: reading
                    .and_then(|reading| reading.total)
                    .or_else(|| file.and_then(|file| file.group_total_bytes)),
                speed_bps: reading.and_then(|reading| reading.speed_bps),
                eta_seconds: reading.and_then(|reading| reading.eta_seconds),
                notice: reading.and_then(|reading| reading.notice.as_deref()),
            })
        });

        let first = first_waiting_position(running.is_some());
        let waiting = self
            .waiting(running.map(|running| &running.id))
            .into_iter()
            .enumerate()
            .map(|(idx, item)| {
                let file = item.shard_info.as_ref();
                row(&RowFacts::waiting(
                    &item.id,
                    first.saturating_add(usize_to_u32_saturating(idx)),
                    file.and_then(ShardInfo::waiting_place),
                    file.and_then(|file| file.group_total_bytes),
                ))
            })
            .collect();

        (active, waiting)
    }

    /// The first pending file of each waiting download, in the order they
    /// will run.
    pub(super) fn waiting(&self, running: Option<&DownloadId>) -> Vec<&QueuedItem> {
        let mut first_files: Vec<&QueuedItem> = Vec::new();
        for item in &self.pending {
            if Some(&item.id) != running && first_files.iter().all(|first| first.id != item.id) {
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
#[path = "placing_tests.rs"]
mod placing_tests;
#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
