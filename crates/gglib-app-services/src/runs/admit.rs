//! How a run is admitted under its id and driven to its end.
//!
//! Shared by both ways a run starts: a chat run through `RunsPort::create`,
//! and a run the daemon's own routes prepared (`local.rs`). A drive races the
//! work against the run's cancellation, records the end, and then hands the
//! ended run and its whole log to whatever asked to see its end.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures_util::FutureExt as _;

use gglib_core::domain::runs::{RunError, RunInfo, RunStatus, is_run_id};
use gglib_core::ports::{RunScope, RunsError};
use tokio::sync::watch;

use super::cell::{RunCell, RunSpec};
use super::local::{RunEnded, RunWork};
use super::registry::RunRegistry;

/// A run whose work, or the handling of its end, panicked.
const PANICKED: (&str, &str) = ("run_panicked", "The run stopped on an internal error.");

fn fixed((code, message): (&str, &str)) -> RunError {
    RunError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

/// Where an id stands when a run is asked for under it.
pub(super) enum Admitted {
    /// The caller's run already has it.
    Existing(RunInfo),
    /// A new run holds it, not yet started.
    New(Arc<RunCell>),
}

/// Counts one live drive, until dropped.
struct Live(watch::Sender<usize>);

impl Drop for Live {
    fn drop(&mut self) {
        self.0.send_modify(|n| *n = n.saturating_sub(1));
    }
}

impl RunRegistry {
    /// The caller's run by `id`, or a new one reserved under it.
    pub(super) fn admit(
        &self,
        scope: RunScope,
        id: &str,
        spec: RunSpec,
        awaits_end: bool,
    ) -> Result<Admitted, RunsError> {
        if !is_run_id(id) {
            return Err(RunsError::InvalidId);
        }
        let mut table = self.lock();
        if table.closed {
            return Err(RunsError::ShuttingDown);
        }
        if let Some(cell) = table.runs.get(id) {
            return if cell.scope == scope {
                Ok(Admitted::Existing(cell.info()))
            } else {
                Err(RunsError::IdTaken)
            };
        }
        // One live reply per conversation: a run is live until its reply is
        // saved, so a new run's user message never races the last one's save.
        if let Some(conversation_id) = spec.conversation_id {
            if let Some(cell) = table.live_on(conversation_id) {
                return Err(RunsError::ConversationBusy {
                    conversation_id,
                    run: cell.id.clone(),
                });
            }
        }
        table.make_room()?;
        let order = table.next_order;
        table.next_order += 1;
        let cell = Arc::new(RunCell::new(
            id,
            scope,
            order,
            spec,
            awaits_end,
            Arc::clone(&self.clock),
        ));
        table.runs.insert(id.to_owned(), Arc::clone(&cell));
        let held = table.runs.len();
        drop(table);
        tracing::debug!(run = %id, runs_held = held, "run created");
        Ok(Admitted::New(cell))
    }

    /// Drop a reserved run that never started, so no run is left behind.
    pub(super) fn unreserve(&self, cell: &Arc<RunCell>) {
        let mut table = self.lock();
        if table
            .runs
            .get(&cell.id)
            .is_some_and(|held| Arc::ptr_eq(held, cell))
        {
            table.runs.remove(&cell.id);
        }
        drop(table);
        cell.drop_now();
        tracing::debug!(run = %cell.id, "a reserved run was dropped unstarted");
    }

    /// Drive `cell` with `work`, then hand its end to `ended`.
    pub(super) fn spawn(&self, cell: Arc<RunCell>, work: RunWork, ended: Option<RunEnded>) {
        self.live.send_modify(|n| *n += 1);
        let live = Live(self.live.clone());
        tokio::spawn(async move {
            let _live = live;
            Self::drive(cell, work, ended).await;
        });
    }

    /// Returns once every run started has ended and its end was handled.
    pub async fn drained(&self) {
        let mut live = self.live.subscribe();
        let _ = live.wait_for(|n| *n == 0).await;
    }

    async fn drive(cell: Arc<RunCell>, work: RunWork, ended: Option<RunEnded>) {
        // Dropping `work` when cancelled is what stops its upstream. A panic
        // in it is caught, so the run still ends and its end is handled.
        let outcome = tokio::select! {
            () = cell.cancel.cancelled() => None,
            outcome = AssertUnwindSafe(work).catch_unwind() => Some(outcome),
        };
        match outcome {
            Some(Ok(Ok(()))) => cell.finish(RunStatus::Completed, None),
            Some(Ok(Err(error))) => cell.finish(RunStatus::Failed, Some(error)),
            Some(Err(_)) => {
                tracing::warn!(run = %cell.id, "a run's work panicked");
                cell.finish(RunStatus::Failed, Some(fixed(PANICKED)))
            }
            None => cell.finish(RunStatus::Cancelled, None),
        };
        let failure = match ended {
            Some(ended) => Self::handle_end(&cell, ended).await,
            None => None,
        };
        cell.settle(failure);
        let info = cell.info();
        tracing::debug!(
            run = %cell.id,
            status = ?info.status,
            events = info.last_seq,
            bytes = cell.bytes(),
            "run ended"
        );
    }

    /// Hand the ended run to `ended`; the error that should fail it, if any.
    async fn handle_end(cell: &RunCell, ended: RunEnded) -> Option<RunError> {
        let handled = AssertUnwindSafe(ended(cell.ending(), cell.frames()))
            .catch_unwind()
            .await;
        handled
            .unwrap_or_else(|_| {
                tracing::warn!(run = %cell.id, "handling a run's end panicked");
                Err(fixed(PANICKED))
            })
            .err()
    }
}
