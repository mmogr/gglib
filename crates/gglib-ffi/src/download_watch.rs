//! One download, as its caller follows it: pulled a step at a time, because
//! an app cannot cancel a call into this library by dropping it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gglib_core::download::DownloadId;
use gglib_core::ports::DownloadManagerPort;
use tokio::sync::Mutex;

use crate::download_feed::{DownloadFeed, Standing};
use crate::runtime::{in_runtime, runtime};

/// How far a download has got.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DownloadProgress {
    /// The download's id: the repository and quantization, as
    /// `owner/name:Q4_K_M`.
    pub id: String,
    /// The bytes of it on the device so far, over every file.
    pub bytes: u64,
    /// The size of every file together, once every size is known.
    pub total: Option<u64>,
    /// Where it is.
    pub state: DownloadState,
}

/// Where a download is.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadState {
    /// Waiting for the download ahead of it.
    Queued,
    /// Fetching its files.
    Running,
    /// Stopped by [`crate::Library::pause_all`], with what it fetched kept;
    /// [`crate::Library::resume_all`] carries on from there.
    Paused,
    /// Every file is on the device, and the model is being added to the
    /// library.
    Verifying,
    /// The model is in the library.
    Done,
    /// It stopped, for `reason`, in gglib's words.
    Failed {
        /// Why.
        reason: String,
    },
}

impl DownloadState {
    /// Whether nothing follows this state.
    const fn is_last(&self) -> bool {
        matches!(self, Self::Done | Self::Failed { .. })
    }
}

/// One download, followed a step at a time.
///
/// [`Self::next`] answers where the download stands each time that changes,
/// starting with where it stands now, and `None` once it is over: after
/// [`DownloadState::Done`] or [`DownloadState::Failed`], or once it was
/// cancelled. A paused download is not over; its watch answers again when it
/// is resumed.
#[derive(uniffi::Object)]
pub struct DownloadWatch {
    id: DownloadId,
    /// The feed's count before the download was asked for. An ending the
    /// feed recorded before then is an earlier attempt's.
    since: u64,
    feed: Arc<DownloadFeed>,
    downloads: Arc<dyn DownloadManagerPort>,
    /// What the watch has answered, held by the one call answering.
    answered: Mutex<Answered>,
    cancelled: AtomicBool,
}

impl DownloadWatch {
    pub(crate) fn new(
        id: DownloadId,
        since: u64,
        feed: Arc<DownloadFeed>,
        downloads: Arc<dyn DownloadManagerPort>,
    ) -> Self {
        Self {
            id,
            since,
            feed,
            downloads,
            answered: Mutex::new(Answered::default()),
            cancelled: AtomicBool::new(false),
        }
    }

    /// What to answer now, from where the feed has the download.
    fn step(&self, answered: &mut Answered) -> Step {
        if answered.over || self.cancelled.load(Ordering::SeqCst) {
            answered.over = true;
            return Step::Over;
        }
        let Some(tracked) = self.feed.tracked(&self.id.to_string()) else {
            return Step::Wait;
        };
        if !tracked.live && tracked.changed_at <= self.since {
            return Step::Wait;
        }
        match tracked.standing {
            Standing::Cancelled => {
                answered.over = true;
                Step::Over
            }
            Standing::At(progress) if answered.last.as_ref() == Some(&progress) => {
                if progress.state.is_last() {
                    answered.over = true;
                    Step::Over
                } else {
                    Step::Wait
                }
            }
            Standing::At(progress) => {
                answered.last = Some(progress.clone());
                Step::Answer(progress)
            }
        }
    }
}

/// What a watch has answered so far.
#[derive(Default)]
struct Answered {
    /// The last progress answered.
    last: Option<DownloadProgress>,
    /// Whether the watch is over: every later call answers `None`.
    over: bool,
}

/// What [`DownloadWatch::next`] does now.
enum Step {
    /// Answer with this progress.
    Answer(DownloadProgress),
    /// Answer `None`: the watch is over.
    Over,
    /// Wait for the feed to change.
    Wait,
}

#[uniffi::export]
impl DownloadWatch {
    /// Where the download stands, once that differs from the last answer,
    /// or `None` once it is over. Calls answer one at a time.
    pub async fn next(self: Arc<Self>) -> Option<DownloadProgress> {
        in_runtime(async move {
            let mut answered = self.answered.lock().await;
            loop {
                // Subscribed before reading, so a change between the read
                // and the wait still wakes it.
                let mut changed = self.feed.subscribe();
                match self.step(&mut answered) {
                    Step::Answer(progress) => return Some(progress),
                    Step::Over => return None,
                    Step::Wait => {}
                }
                if changed.changed().await.is_err() {
                    return None;
                }
            }
        })
        .await
    }

    /// Stop the download, keeping what it fetched for the next attempt to
    /// carry on from, and end this watch: [`Self::next`] answers `None`.
    /// Returns at once; the download manager is told on the library's own
    /// threads.
    pub fn cancel(&self) {
        if self.cancelled.swap(true, Ordering::SeqCst) {
            return;
        }
        let id = self.id.clone();
        let feed = Arc::clone(&self.feed);
        let downloads = Arc::clone(&self.downloads);
        runtime().spawn(async move {
            // A paused download has nothing running to stop.
            if !feed.unpause(&id.to_string()) {
                let _ = downloads.cancel_download(&id).await;
            }
        });
        self.feed.poke();
    }
}
