//! A run the daemon's own routes start with work they prepared, such as an
//! agent run, whose loop is composed where its request is read.
//!
//! Nothing here is on `RunsPort`, which is all the proxy's door holds, so
//! only this machine starts such a run. The id is reserved first and the
//! work started after, so a caller can do what must happen once per run in
//! between (writing the user's message) without a repeat of the same id
//! doing it twice. A reservation dropped unstarted leaves no run behind.

use std::sync::Arc;

use futures_util::future::BoxFuture;
use gglib_core::domain::runs::{RunError, RunInfo, is_run_id};
use gglib_core::ports::{RunScope, RunsError};

use super::admit::Admitted;
use super::cell::{RunCell, RunSpec};
use super::executor::RunLog;
use super::registry::RunRegistry;

/// A run's work: `Ok` completes the run and `Err` fails it. It is dropped
/// when the run is cancelled, which is what stops it.
pub type RunWork = BoxFuture<'static, Result<(), RunError>>;

/// Called once a run has ended, whatever the end, with its state and every
/// frame it logged. Its readers are given the end only after it returns; an
/// `Err` makes the run `failed` with that error.
pub type RunEnded =
    Box<dyn FnOnce(RunInfo, Vec<Arc<str>>) -> BoxFuture<'static, Result<(), RunError>> + Send>;

/// Where a reserved id stands.
pub enum Reservation<'a> {
    /// This machine's run already has the id; nothing was reserved.
    Existing(RunInfo),
    /// A new run holds the id, queued until it is started.
    New(Reserved<'a>),
}

/// A new run that has not started. Dropped unstarted, it is removed.
pub struct Reserved<'a> {
    registry: &'a RunRegistry,
    cell: Arc<RunCell>,
    started: bool,
}

impl Reserved<'_> {
    /// The run as it stands.
    #[must_use]
    pub fn info(&self) -> RunInfo {
        self.cell.info()
    }

    /// Start the run: `work` is handed the run's log, and `ended` is called
    /// once it ends. Returns the run as it stood when started.
    pub fn start(mut self, work: impl FnOnce(RunLog) -> RunWork, ended: RunEnded) -> RunInfo {
        self.started = true;
        let cell = Arc::clone(&self.cell);
        let info = cell.info();
        let work = work(RunLog::new(Arc::clone(&cell)));
        self.registry.spawn(cell, work, Some(ended));
        info
    }
}

impl Drop for Reserved<'_> {
    fn drop(&mut self) {
        if !self.started {
            self.registry.unreserve(&self.cell);
        }
    }
}

impl RunRegistry {
    /// This machine's run by `id`, if there is one.
    ///
    /// # Errors
    ///
    /// [`RunsError::InvalidId`], and [`RunsError::IdTaken`] when a paired
    /// device's run has the id.
    pub fn existing(&self, id: &str) -> Result<Option<RunInfo>, RunsError> {
        if !is_run_id(id) {
            return Err(RunsError::InvalidId);
        }
        match self.lock().runs.get(id) {
            None => Ok(None),
            Some(cell) if cell.scope == RunScope::Local => Ok(Some(cell.info())),
            Some(_) => Err(RunsError::IdTaken),
        }
    }

    /// Reserve `id` for a run of this machine, or answer with the run that
    /// already has it.
    ///
    /// # Errors
    ///
    /// As `RunsPort::create`, less the body check, and
    /// [`RunsError::ConversationBusy`] while another run's reply to the
    /// same conversation is not yet saved.
    pub fn reserve(&self, id: &str, spec: RunSpec) -> Result<Reservation<'_>, RunsError> {
        Ok(match self.admit(RunScope::Local, id, spec, true)? {
            Admitted::Existing(info) => Reservation::Existing(info),
            Admitted::New(cell) => Reservation::New(Reserved {
                registry: self,
                cell,
                started: false,
            }),
        })
    }
}
