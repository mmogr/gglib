//! A download fetched on its own, without the queue.
//!
//! `gglib model upgrade` fetches a model's files itself, one after another.
//! It is still one download and reads as one row, made of the queue's own
//! parts: its files are placed by [`group_files`], its bytes, speed and time
//! remaining are kept by a [`GroupMeter`], a note stands in for progress by
//! [`ProgressUpdate`]'s rule, and the row is [`running_row`]'s. Nothing here
//! words a row or measures a transfer.

use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use gglib_core::download::{DownloadError, DownloadId, DownloadPhase, DownloadRow, ShardInfo};
use gglib_core::ports::ResolvedFile;

use crate::cli_exec::NoticeCallback;
use crate::executor::ProgressCallback;
use crate::manager::{GroupMeter, ProgressUpdate};
use crate::queue::group_items::group_files;
use crate::queue::{Reading, Running, running_row};

/// Sink for the row of a download fetched without the queue.
///
/// It is handed the row as it stands four times a second while a file is
/// fetched, and once more as each file lands.
pub type RowCallback = Arc<dyn Fn(&DownloadRow) + Send + Sync>;

/// One download, fetched a file at a time by its caller.
pub(crate) struct SoloDownload {
    id: DownloadId,
    /// Its files, each with its place in the group, in the order fetched.
    files: Vec<ShardInfo>,
    state: Mutex<State>,
}

struct State {
    /// The download's bytes, speed and time remaining over every file.
    meter: GroupMeter,
    /// The file being fetched, or the next one between two, by its number
    /// in `files`.
    index: usize,
    /// The latest progress of the file being fetched, and any note on it.
    update: ProgressUpdate,
}

impl State {
    /// Feed the meter the file's latest progress, and read it.
    fn sample(&mut self, now: Instant) -> Reading {
        self.meter
            .observe(self.update.progress, self.update.notice.as_deref(), now);
        self.meter.reading()
    }
}

impl SoloDownload {
    /// The download `id` of `files`, about to fetch the first of them, with
    /// its meter's baseline at `now`.
    pub(crate) fn new(id: DownloadId, files: &[ResolvedFile], now: Instant) -> Self {
        let files = group_files(files);
        let meter = GroupMeter::for_group(files.first(), now);
        Self {
            id,
            files,
            state: Mutex::new(State {
                meter,
                index: 0,
                update: ProgressUpdate::default(),
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A sink for the progress of the file being fetched.
    pub(crate) fn progress(self: &Arc<Self>) -> ProgressCallback {
        let solo = Arc::clone(self);
        Arc::new(move |progress| solo.state().update.advance(progress))
    }

    /// A sink for notes on the file being fetched. A note is the row's
    /// status until bytes arrive again.
    pub(crate) fn notices(self: &Arc<Self>) -> NoticeCallback {
        let solo = Arc::clone(self);
        Arc::new(move |message: &str| solo.state().update.note(message))
    }

    /// The file being fetched is in place: its counts join those of the
    /// files before it, and the next file, when there is one, starts from
    /// nothing.
    pub(crate) fn file_done(&self, now: Instant) {
        let mut state = self.state();
        let last = std::mem::take(&mut state.update);
        state.meter.observe(last.progress, None, now);
        state.meter.file_done();
        if state.index + 1 < self.files.len() {
            state.index += 1;
        }
    }

    /// The download's row at `now`. Each call is one sample for the meter,
    /// so it is called on a steady tick, moved or not.
    pub(crate) fn row(&self, now: Instant) -> DownloadRow {
        let mut state = self.state();
        let reading = state.sample(now);
        let file = self.files.get(state.index).cloned();
        drop(state);

        let running = Running {
            id: self.id.clone(),
            phase: DownloadPhase::Downloading,
            file,
        };
        running_row(&running, Some(&reading))
    }
}

/// Fetch the download `id` of `files` one file at a time, and hand `rows`
/// its row as it goes: once every `every` while a file is fetched, and as
/// each file lands.
///
/// `fetch` is given a file's number in `files` and the sinks for its
/// progress and its notes. The first file it fails on ends the download
/// with that error, and the files after it are not fetched.
pub(crate) async fn fetch_solo<F, Fut>(
    id: DownloadId,
    files: &[ResolvedFile],
    rows: Option<&RowCallback>,
    every: Duration,
    mut fetch: F,
) -> Result<(), DownloadError>
where
    F: FnMut(usize, ProgressCallback, NoticeCallback) -> Fut,
    Fut: Future<Output = Result<(), DownloadError>>,
{
    let solo = Arc::new(SoloDownload::new(id, files, Instant::now()));
    let show = || {
        if let Some(rows) = rows {
            rows(&solo.row(Instant::now()));
        }
    };

    let fetch_all = async {
        for index in 0..files.len() {
            fetch(index, solo.progress(), solo.notices()).await?;
            solo.file_done(Instant::now());
            show();
        }
        Ok(())
    };
    tokio::pin!(fetch_all);

    let mut tick = tokio::time::interval(every);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;

            result = &mut fetch_all => return result,

            _ = tick.tick() => show(),
        }
    }
}

#[cfg(test)]
#[path = "solo_tests.rs"]
mod tests;
