//! When a download monitor is done.
//!
//! Three loops watch a queue: the one polling the daemon, and the two in
//! this process, with and without a terminal. They all ask the same
//! question of each snapshot, and [`MonitorState::step`] is the one place
//! it is answered. [`QueueWatch`] is how each of them asks it: draw the
//! snapshot, take the step, and report on the way out.

use std::collections::BTreeSet;
use std::sync::Arc;

use gglib_core::download::{DownloadId, DownloadOutcome, FinishedDownload, QueueSnapshot};

use crate::console::CliConsole;

use super::board::DownloadBoard;

/// Which downloads a monitor is waiting for.
#[derive(Debug, Clone)]
enum Watch {
    /// Every download in the queue: this process owns the queue.
    Everything,
    /// One download, by the ID the daemon gave it when it was queued. The
    /// queue is the daemon's, and may hold other people's downloads, another
    /// quantization of the same repository among them.
    Download(String),
}

impl Watch {
    /// Whether the download `id` is one this monitor is waiting for.
    fn covers(&self, id: &str) -> bool {
        match self {
            Self::Everything => true,
            Self::Download(mine) => id == mine,
        }
    }
}

/// What a monitor does after a snapshot.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Its downloads are still in the queue, or have not reached it yet.
    Continue,
    /// Its downloads have left the queue, ending as listed. The list is
    /// empty when their outcome is no longer in the snapshot.
    Exit(Vec<FinishedDownload>),
}

/// A monitor's memory between snapshots.
#[derive(Debug)]
pub(crate) struct MonitorState {
    watch: Watch,
    /// The downloads of its own known to have reached the queue: those that
    /// have been rows of some snapshot, and the one the daemon said it
    /// queued.
    seen: BTreeSet<String>,
}

impl MonitorState {
    /// A monitor of every download in the queue.
    pub(crate) const fn everything() -> Self {
        Self {
            watch: Watch::Everything,
            seen: BTreeSet::new(),
        }
    }

    /// A monitor of the download `id`, which the daemon has queued: it
    /// answered the queue request with this ID, so the download has reached
    /// the queue whether or not a snapshot has shown it yet.
    pub(crate) fn download(id: &DownloadId) -> Self {
        Self {
            watch: Watch::Download(id.to_string()),
            seen: BTreeSet::from([id.to_string()]),
        }
    }

    /// Take the next snapshot.
    ///
    /// While a download of its own is a row, running, between two files or
    /// waiting, the monitor goes on. With none, it exits once it has
    /// something to say: an outcome of its own in the finished list, which
    /// covers a download that ended before the first snapshot, or a download
    /// known to have reached the queue that is no longer in it. Until then
    /// nothing has reached the queue yet, and it goes on.
    pub(crate) fn step(&mut self, snapshot: &QueueSnapshot) -> Step {
        let mut live = false;
        for row in snapshot.rows().filter(|row| self.watch.covers(&row.id)) {
            self.seen.insert(row.id.clone());
            live = true;
        }
        if live {
            return Step::Continue;
        }

        // A download queued again loses its old entry as it is queued, so an
        // entry under a watched ID is how this run of it ended.
        let ended: Vec<FinishedDownload> = snapshot
            .finished
            .iter()
            .filter(|ended| self.watch.covers(&ended.id))
            .cloned()
            .collect();
        if ended.is_empty() && self.seen.is_empty() {
            return Step::Continue;
        }
        Step::Exit(ended)
    }
}

/// The board and a monitor's memory, which every loop drives the same way.
pub(crate) struct QueueWatch {
    board: DownloadBoard,
    state: MonitorState,
}

impl QueueWatch {
    /// A watch of every download in the queue, drawn on `console`.
    pub(crate) fn everything(console: Arc<CliConsole>) -> Self {
        Self {
            board: DownloadBoard::new(console),
            state: MonitorState::everything(),
        }
    }

    /// A watch of the download `id`, drawn on `console`. The board still
    /// draws every row of the queue.
    pub(crate) fn download(console: Arc<CliConsole>, id: &DownloadId) -> Self {
        Self {
            board: DownloadBoard::new(console),
            state: MonitorState::download(id),
        }
    }

    /// Draw `snapshot`. When the watched downloads have ended, clear the
    /// board, print how each ended and answer the same list. `None` is to
    /// go on.
    pub(crate) fn take(&mut self, snapshot: &QueueSnapshot) -> Option<Vec<FinishedDownload>> {
        self.board.sync(snapshot);
        match self.state.step(snapshot) {
            Step::Continue => None,
            Step::Exit(ended) => {
                self.board.clear();
                self.board.report(&ended);
                Some(ended)
            }
        }
    }

    /// Take the bars off the screen, for a loop that stops before its
    /// downloads have ended.
    pub(crate) fn clear(&mut self) {
        self.board.clear();
    }
}

/// Why the downloads that `ended` this way did not all arrive: the words of
/// the first that failed or was cancelled. `None` when every one completed.
pub(crate) fn failure(ended: &[FinishedDownload]) -> Option<String> {
    ended
        .iter()
        .find(|ended| !matches!(ended.outcome, DownloadOutcome::Completed { .. }))
        .map(|ended| ended.text.clone())
}

/// What watching the download `id` comes to, given how it `ended`: `Ok`
/// when it completed, and otherwise why not.
///
/// An empty list is not a success. Every download leaves an outcome, so
/// this one's has gone from the few the queue keeps, or was cleared.
pub(crate) fn download_result(id: &DownloadId, ended: &[FinishedDownload]) -> Result<(), String> {
    if ended.is_empty() {
        return Err(format!(
            "{id} left the download queue and how it ended is no longer recorded"
        ));
    }
    failure(ended).map_or(Ok(()), Err)
}

#[cfg(test)]
#[path = "monitor_tests.rs"]
pub(super) mod tests;
#[cfg(test)]
#[path = "watch_tests.rs"]
mod watch_tests;
