//! How a run is admitted under its id and driven to its end.
//!
//! Shared by both ways a run starts: a chat run through `RunsPort::create`,
//! and a run the daemon's own routes prepared (`local.rs`). A drive races the
//! work against the run's cancellation, records the end, and then hands the
//! ended run and its whole log to whatever asked to see its end.

use std::sync::Arc;

use gglib_core::domain::runs::{RunInfo, RunStatus, is_run_id};
use gglib_core::ports::{RunScope, RunsError};
use tokio::sync::watch;

use super::cell::{RunCell, RunSpec};
use super::local::{RunEnded, RunWork};
use super::registry::RunRegistry;

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
        table.make_room()?;
        let order = table.next_order;
        table.next_order += 1;
        let cell = Arc::new(RunCell::new(
            id,
            scope,
            order,
            spec,
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
        // Dropping `work` when cancelled is what stops its upstream.
        let outcome = tokio::select! {
            () = cell.cancel.cancelled() => None,
            outcome = work => Some(outcome),
        };
        match outcome {
            Some(Ok(())) => cell.finish(RunStatus::Completed, None),
            Some(Err(error)) => cell.finish(RunStatus::Failed, Some(error)),
            None => cell.finish(RunStatus::Cancelled, None),
        };
        let info = cell.info();
        tracing::debug!(
            run = %cell.id,
            status = ?info.status,
            events = info.last_seq,
            bytes = cell.bytes(),
            "run ended"
        );
        if let Some(ended) = ended {
            ended(info, cell.frames()).await;
        }
    }
}
