//! One run: its status, its log, and the signal its readers wait on.
//!
//! The log is a vector read by index, and a reader is woken by a `watch`
//! whose value is only a version. A reader that falls behind reads the
//! vector from where it was, so no reader ever misses an event, which a
//! broadcast channel would not promise.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gglib_core::domain::runs::{RunError, RunInfo, RunKind, RunStatus};
use gglib_core::ports::RunScope;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::Clock;

/// The most a run's log may hold, in bytes of frame text.
pub(super) const LOG_LIMIT: usize = 8 * 1024 * 1024;

/// An ended run is dropped this long after a reader of its own scope read it
/// to the end.
pub(super) const KEEP_AFTER_READ_MS: u64 = 10 * 60 * 1000;

/// An ended run is dropped this long after it ended, read or not.
pub(super) const KEEP_AFTER_END_MS: u64 = 24 * 60 * 60 * 1000;

/// The log refused a frame: the run has ended, and the executor should stop.
#[derive(Debug)]
pub struct Stopped;

/// What a run is, fixed when it is created.
pub struct RunSpec {
    /// What it produces.
    pub kind: RunKind,
    /// The model it is sent to, when one is named.
    pub model: Option<String>,
    /// The saved conversation its transcript is written to, if any.
    pub conversation_id: Option<i64>,
}

/// What a reader at `cursor` should do next.
pub(super) enum Step {
    /// Frames after the cursor, in order.
    Frames(Vec<Arc<str>>),
    /// Every frame is read and the run has ended.
    End(RunInfo),
    /// The run is gone; end without an `End`.
    Dropped,
    /// Nothing new yet.
    Wait,
}

struct State {
    info: RunInfo,
    frames: Vec<Arc<str>>,
    bytes: usize,
    dropped: bool,
    read_to_end_at_ms: Option<u64>,
}

pub(super) struct RunCell {
    pub(super) id: String,
    pub(super) scope: RunScope,
    /// Creation order, for "oldest" and "newest" among runs created in the
    /// same millisecond.
    pub(super) order: u64,
    pub(super) cancel: CancellationToken,
    state: Mutex<State>,
    changed: watch::Sender<u64>,
    clock: Clock,
}

impl RunCell {
    pub(super) fn new(id: &str, scope: RunScope, order: u64, spec: RunSpec, clock: Clock) -> Self {
        let info = RunInfo {
            id: id.to_owned(),
            kind: spec.kind,
            status: RunStatus::Queued,
            model: spec.model,
            device: scope.device().map(str::to_owned),
            created_at_ms: clock(),
            finished_at_ms: None,
            conversation_id: spec.conversation_id,
            last_seq: 0,
            error: None,
        };
        Self {
            id: id.to_owned(),
            scope,
            order,
            cancel: CancellationToken::new(),
            state: Mutex::new(State {
                info,
                frames: Vec::new(),
                bytes: 0,
                dropped: false,
                read_to_end_at_ms: None,
            }),
            changed: watch::Sender::new(0),
            clock,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wake(&self) {
        self.changed.send_modify(|v| *v = v.wrapping_add(1));
    }

    pub(super) fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }

    pub(super) fn info(&self) -> RunInfo {
        self.lock().info.clone()
    }

    /// Every frame logged, for what reads the whole log once it has ended.
    pub(super) fn frames(&self) -> Vec<Arc<str>> {
        self.lock().frames.clone()
    }

    /// The log's size in bytes, for the line that says a run ended.
    pub(super) fn bytes(&self) -> usize {
        self.lock().bytes
    }

    pub(super) fn is_ended(&self) -> bool {
        self.lock().info.status.is_terminal()
    }

    /// The upstream answered: `queued` becomes `in_progress`.
    pub(crate) fn started(&self) {
        let mut state = self.lock();
        if state.info.status == RunStatus::Queued {
            state.info.status = RunStatus::InProgress;
            drop(state);
            self.wake();
        }
    }

    /// Log one frame. A frame that would take the log past [`LOG_LIMIT`]
    /// fails the run with `log_full` and cancels its upstream instead.
    pub(crate) fn append(&self, frame: String) -> Result<(), Stopped> {
        let mut state = self.lock();
        if state.info.status.is_terminal() {
            return Err(Stopped);
        }
        if state.bytes + frame.len() > LOG_LIMIT {
            let error = RunError {
                code: "log_full".to_owned(),
                message: "The reply passed the 8 MB a run may hold.".to_owned(),
            };
            Self::end(&mut state, RunStatus::Failed, Some(error), (self.clock)());
            drop(state);
            self.cancel.cancel();
            self.wake();
            return Err(Stopped);
        }
        state.bytes += frame.len();
        state.frames.push(Arc::from(frame));
        state.info.last_seq = u32::try_from(state.frames.len()).unwrap_or(u32::MAX);
        drop(state);
        self.wake();
        Ok(())
    }

    fn end(state: &mut State, status: RunStatus, error: Option<RunError>, now: u64) {
        state.info.status = status;
        state.info.error = error;
        state.info.finished_at_ms = Some(now);
    }

    /// End the run, unless it has ended already. Returns whether this call
    /// ended it.
    pub(super) fn finish(&self, status: RunStatus, error: Option<RunError>) -> bool {
        let mut state = self.lock();
        if state.info.status.is_terminal() {
            return false;
        }
        Self::end(&mut state, status, error, (self.clock)());
        drop(state);
        self.wake();
        true
    }

    /// Cancel: end as `cancelled` if still going, and stop the upstream.
    pub(super) fn cancel(&self) -> RunInfo {
        self.finish(RunStatus::Cancelled, None);
        self.cancel.cancel();
        self.info()
    }

    /// Cancel and mark gone, so every reader ends.
    pub(super) fn drop_now(&self) {
        self.finish(RunStatus::Cancelled, None);
        self.cancel.cancel();
        self.lock().dropped = true;
        self.wake();
    }

    /// A reader of the run's own scope reached its end.
    pub(super) fn mark_read(&self) {
        let now = (self.clock)();
        self.lock().read_to_end_at_ms.get_or_insert(now);
    }

    /// Whether retention has run out for this run at `now`.
    pub(super) fn expired(&self, now: u64) -> bool {
        let (finished, read) = {
            let state = self.lock();
            (state.info.finished_at_ms, state.read_to_end_at_ms)
        };
        let Some(ended) = finished else {
            return false;
        };
        let read_out = read.is_some_and(|read| now >= read.saturating_add(KEEP_AFTER_READ_MS));
        read_out || now >= ended.saturating_add(KEEP_AFTER_END_MS)
    }

    /// What a reader that has seen `cursor` frames should do next.
    pub(super) fn step(&self, cursor: usize) -> Step {
        let state = self.lock();
        if state.dropped {
            return Step::Dropped;
        }
        if cursor < state.frames.len() {
            return Step::Frames(state.frames[cursor..].to_vec());
        }
        if state.info.status.is_terminal() {
            return Step::End(state.info.clone());
        }
        Step::Wait
    }
}
