//! What the download manager says, kept per download for the watches that
//! read it.
//!
//! [`DownloadFeed`] is the event emitter the library's core is built with.
//! It reads each queue snapshot the manager publishes, in the order the
//! manager publishes them, and keeps where each download stands: its row
//! while the download is waiting or running, and how it ended once the row
//! leaves for the snapshot's finished list. Only a download seen waiting or
//! running ends here, so a finished entry left from an earlier attempt is
//! never taken for this one's ending. A download paused by
//! [`DownloadFeed::pause`] ends cancelled in the manager and stands
//! [`DownloadState::Paused`] here; a resume takes only one that stands so,
//! never one that ended done or failed, whenever the pause was noted.
//!
//! The manager emits from inside its publishing, so `emit` only takes a
//! short lock and wakes the watches; it never waits on anything.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use gglib_core::download::{
    DownloadEvent, DownloadOutcome, DownloadPhase, DownloadRow, QueueSnapshot,
};
use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use tokio::sync::watch;

use crate::download_watch::{DownloadProgress, DownloadState};

/// Where one download stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Standing {
    /// Waiting, running, paused, or ended done or failed.
    At(DownloadProgress),
    /// Cancelled, other than by a pause: a watch of it ends with nothing
    /// more to say.
    Cancelled,
}

/// A download's standing, and when the feed last changed it.
#[derive(Debug, Clone)]
pub(crate) struct Tracked {
    pub(crate) standing: Standing,
    /// The feed's count when the standing was last set.
    pub(crate) changed_at: u64,
    /// Whether the last snapshot had the download waiting or running.
    pub(crate) live: bool,
}

/// A download [`DownloadFeed::pause`] stopped, to be queued again by
/// [`DownloadFeed::take_paused`]'s caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Paused {
    /// The download's id, as the manager gave it.
    pub(crate) id: String,
    /// The repository, `owner/name`.
    pub(crate) repo: String,
    /// The quantization, when the download names one.
    pub(crate) quantization: Option<String>,
}

#[derive(Default)]
struct State {
    downloads: HashMap<String, Tracked>,
    /// Paused downloads, in the order they stood in the queue.
    paused: Vec<Paused>,
}

/// The download manager's event emitter, and the record the watches read.
pub(crate) struct DownloadFeed {
    state: Mutex<State>,
    /// Counts every change, and wakes every watch waiting for one.
    count: watch::Sender<u64>,
}

impl DownloadFeed {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(State::default()),
            count: watch::Sender::new(0),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The feed's count of changes so far.
    pub(crate) fn count(&self) -> u64 {
        *self.count.borrow()
    }

    /// A receiver woken at the next change.
    pub(crate) fn subscribe(&self) -> watch::Receiver<u64> {
        self.count.subscribe()
    }

    /// Wake every watch, with nothing changed: one that was cancelled reads
    /// that it was.
    pub(crate) fn poke(&self) {
        self.count.send_modify(|count| *count += 1);
    }

    /// Where download `id` stands, if the feed has heard of it.
    pub(crate) fn tracked(&self, id: &str) -> Option<Tracked> {
        self.state().downloads.get(id).cloned()
    }

    /// Note that each download of `rows` is being paused, so the
    /// cancellation the manager reports for it reads as a pause.
    pub(crate) fn pause(&self, rows: &[DownloadRow]) {
        let mut state = self.state();
        for row in rows {
            if state.paused.iter().all(|paused| paused.id != row.id) {
                state.paused.push(Paused {
                    id: row.id.clone(),
                    repo: row.model_id.clone(),
                    quantization: row.quantization.clone(),
                });
            }
        }
    }

    /// The paused downloads to queue again, in their queue order, forgotten
    /// as paused: each that has stopped and stands paused, or that the feed
    /// has never heard of; one that ended otherwise is dropped. One still
    /// winding down from its cancel stays paused: queued again now it would
    /// attach to the transfer that is ending, so its ending reads as a pause
    /// and a later call takes it.
    pub(crate) fn take_paused(&self) -> Vec<Paused> {
        let mut state = self.state();
        let State { downloads, paused } = &mut *state;
        let mut stopped = std::mem::take(paused);
        paused.extend(stopped.extract_if(.., |entry| is_live(downloads, &entry.id)));
        stopped.retain(|entry| stands_paused(downloads, &entry.id));
        drop(state);
        stopped
    }

    /// Wait until no download of `ids` is waiting or running in the last
    /// snapshot the feed read: each has ended, and the feed has read how.
    pub(crate) async fn until_stopped(&self, ids: &[String]) {
        loop {
            // Subscribed before reading, so a change between the read and
            // the wait still wakes it.
            let mut changed = self.subscribe();
            let live = ids.iter().any(|id| is_live(&self.state().downloads, id));
            if !live || changed.changed().await.is_err() {
                return;
            }
        }
    }

    /// Forget download `id` as paused. Answers whether it was.
    pub(crate) fn unpause(&self, id: &str) -> bool {
        let mut state = self.state();
        let before = state.paused.len();
        state.paused.retain(|paused| paused.id != id);
        state.paused.len() != before
    }

    /// Record that download `id` failed for `reason` outside the manager,
    /// as when it could not be queued again.
    pub(crate) fn fail(&self, id: &str, reason: String) {
        let mut state = self.state();
        let bytes = match state.downloads.get(id) {
            Some(Tracked {
                standing: Standing::At(progress),
                ..
            }) => (progress.bytes, progress.total),
            _ => (0, None),
        };
        let progress = DownloadProgress {
            id: id.to_owned(),
            bytes: bytes.0,
            total: bytes.1,
            state: DownloadState::Failed { reason },
        };
        self.set(&mut state, id, Standing::At(progress), false);
        drop(state);
    }

    /// Set download `id`'s standing, counting the change.
    fn set(&self, state: &mut State, id: &str, standing: Standing, live: bool) {
        let mut changed_at = 0;
        self.count.send_modify(|count| {
            *count += 1;
            changed_at = *count;
        });
        state.downloads.insert(
            id.to_owned(),
            Tracked {
                standing,
                changed_at,
                live,
            },
        );
    }

    /// Read one snapshot: each row is where its download stands, and a
    /// download last seen live that has left the rows stands as its finished
    /// entry says it ended.
    fn read(&self, snapshot: &QueueSnapshot) {
        let mut state = self.state();
        let ended: Vec<String> = state
            .downloads
            .iter()
            .filter(|(id, tracked)| tracked.live && snapshot.rows().all(|row| &row.id != *id))
            .map(|(id, _)| id.clone())
            .collect();
        for row in snapshot.rows() {
            let progress = progress_of(row);
            let unchanged = state.downloads.get(&row.id).is_some_and(|tracked| {
                tracked.live && tracked.standing == Standing::At(progress.clone())
            });
            if !unchanged {
                self.set(&mut state, &row.id, Standing::At(progress), true);
            }
        }
        for id in ended {
            let Some(finished) = snapshot.finished.iter().find(|entry| entry.id == id) else {
                continue;
            };
            let last = match state.downloads.get(&id).map(|tracked| &tracked.standing) {
                Some(Standing::At(progress)) => progress.clone(),
                _ => continue,
            };
            let paused = state.paused.iter().any(|paused| paused.id == id);
            let standing = ending(&finished.outcome, last, paused);
            self.set(&mut state, &id, standing, false);
        }
        drop(state);
    }
}

/// Whether download `id` is waiting or running in the last snapshot read.
fn is_live(downloads: &HashMap<String, Tracked>, id: &str) -> bool {
    downloads.get(id).is_some_and(|tracked| tracked.live)
}

/// Whether download `id` stands paused, or the feed has never heard of it.
fn stands_paused(downloads: &HashMap<String, Tracked>, id: &str) -> bool {
    downloads.get(id).is_none_or(|tracked| {
        matches!(&tracked.standing, Standing::At(at) if at.state == DownloadState::Paused)
    })
}

/// Where a download stands that ended with `outcome`, from where it stood
/// `last`; `paused` when [`DownloadFeed::pause`] stopped it.
fn ending(outcome: &DownloadOutcome, last: DownloadProgress, paused: bool) -> Standing {
    match outcome {
        DownloadOutcome::Completed { .. } => Standing::At(DownloadProgress {
            bytes: last.total.unwrap_or(last.bytes),
            state: DownloadState::Done,
            ..last
        }),
        DownloadOutcome::Failed { error } => Standing::At(DownloadProgress {
            state: DownloadState::Failed {
                reason: error.clone(),
            },
            ..last
        }),
        DownloadOutcome::Cancelled if paused => Standing::At(DownloadProgress {
            state: DownloadState::Paused,
            ..last
        }),
        DownloadOutcome::Cancelled => Standing::Cancelled,
    }
}

/// Where a waiting or running download's row says it stands.
fn progress_of(row: &DownloadRow) -> DownloadProgress {
    let state = match row.phase {
        DownloadPhase::Queued => DownloadState::Queued,
        DownloadPhase::Downloading => DownloadState::Running,
        DownloadPhase::Finalizing | DownloadPhase::Registering => DownloadState::Verifying,
    };
    DownloadProgress {
        id: row.id.clone(),
        bytes: row.downloaded_bytes,
        total: row.total_bytes,
        state,
    }
}

impl AppEventEmitter for DownloadFeed {
    fn emit(&self, event: AppEvent) {
        if let AppEvent::Download {
            event: DownloadEvent::QueueSnapshot(snapshot),
        } = event
        {
            self.read(&snapshot);
        }
    }
}

#[cfg(test)]
#[path = "download_feed_tests.rs"]
mod tests;
