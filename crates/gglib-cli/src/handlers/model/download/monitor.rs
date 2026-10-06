//! When a download monitor is done.
//!
//! Three loops watch a queue: the one polling the daemon, and the two in
//! this process, with and without a terminal. They all ask the same
//! question of each snapshot, and [`MonitorState::step`] is the one place
//! it is answered. [`QueueWatch`] is how each of them asks it: draw the
//! snapshot, take the step, and report on the way out.

use std::collections::BTreeSet;
use std::sync::Arc;

use gglib_core::download::{DownloadOutcome, FinishedDownload, QueueSnapshot};

use crate::console::CliConsole;

use super::board::DownloadBoard;

/// Which downloads a monitor is waiting for.
#[derive(Debug, Clone)]
enum Watch {
    /// Every download in the queue: this process owns the queue.
    Everything,
    /// The downloads of one repository. The queue is the daemon's, and may
    /// hold other people's.
    Model(String),
}

impl Watch {
    /// Whether the download `id` is one this monitor is waiting for.
    fn covers(&self, id: &str) -> bool {
        match self {
            Self::Everything => true,
            Self::Model(model_id) => id
                .strip_prefix(model_id.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(':')),
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
    /// The downloads of its own that have been rows of some snapshot.
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

    /// A monitor of the downloads of the repository `model_id`.
    pub(crate) fn model(model_id: &str) -> Self {
        Self {
            watch: Watch::Model(model_id.to_string()),
            seen: BTreeSet::new(),
        }
    }

    /// Take the next snapshot.
    ///
    /// While a download of its own is a row, running, between two files or
    /// waiting, the monitor goes on. With none, it exits once it has
    /// something to say: an outcome of its own in the finished list, which
    /// covers a download that failed before the first snapshot, or a
    /// download it saw earlier and no longer does. Until then nothing has
    /// reached the queue yet, and it goes on.
    pub(crate) fn step(&mut self, snapshot: &QueueSnapshot) -> Step {
        let mut live = false;
        for row in snapshot.rows().filter(|row| self.watch.covers(&row.id)) {
            self.seen.insert(row.id.clone());
            live = true;
        }
        if live {
            return Step::Continue;
        }

        let ended: Vec<FinishedDownload> = snapshot
            .finished
            .iter()
            .filter(|ended| self.is_its_outcome(&ended.id))
            .cloned()
            .collect();
        if ended.is_empty() && self.seen.is_empty() {
            return Step::Continue;
        }
        Step::Exit(ended)
    }

    /// Whether the finished entry `id` is how a download of this monitor's
    /// ended.
    ///
    /// The daemon keeps the outcomes of earlier runs, and an earlier
    /// download of the same repository is not this run's. So a monitor of
    /// one repository that has seen rows counts the outcomes of those rows
    /// alone. One that has seen none takes every outcome of its repository:
    /// its download ended before the first snapshot. A monitor of the whole
    /// queue owns the queue, and every outcome in it.
    fn is_its_outcome(&self, id: &str) -> bool {
        match &self.watch {
            Watch::Everything => true,
            Watch::Model(_) => {
                self.watch.covers(id) && (self.seen.is_empty() || self.seen.contains(id))
            }
        }
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

    /// A watch of the downloads of the repository `model_id`, drawn on
    /// `console`. The board still draws every row of the queue.
    pub(crate) fn model(console: Arc<CliConsole>, model_id: &str) -> Self {
        Self {
            board: DownloadBoard::new(console),
            state: MonitorState::model(model_id),
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

/// Why the downloads that `ended` this way did not all arrive: the first
/// that failed or was cancelled, in words. `None` when every one completed.
pub(crate) fn failure(ended: &[FinishedDownload]) -> Option<String> {
    ended.iter().find_map(|ended| match &ended.outcome {
        DownloadOutcome::Completed { .. } => None,
        DownloadOutcome::Failed { error } => {
            Some(format!("download failed: {} \u{2014} {error}", ended.title))
        }
        DownloadOutcome::Cancelled => Some(format!("download cancelled: {}", ended.title)),
    })
}

/// What watching the downloads of `model_id` comes to, given how they
/// `ended`: `Ok` when every one completed, and otherwise why not.
///
/// An empty list is not a success. The download left the queue and nothing
/// says how: it was taken off while it waited, or its outcome is no longer
/// among the few the queue keeps.
pub(crate) fn model_result(model_id: &str, ended: &[FinishedDownload]) -> Result<(), String> {
    if ended.is_empty() {
        return Err(format!(
            "{model_id} left the download queue and how it ended is not recorded"
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
