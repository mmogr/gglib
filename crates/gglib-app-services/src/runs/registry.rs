//! The registry: every run this daemon holds, the scope rule, the limits and
//! retention.
//!
//! Shaped like the download manager: a map under a mutex, a cancellation
//! token per job. Retention is swept at the start of every call, against the
//! registry's clock, so a run is gone by the first call after its time is up.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gglib_core::domain::runs::{RunInfo, RunList, RunStatus, is_run_id};
use gglib_core::ports::{Created, RunEvents, RunScope, RunsError, RunsPort};
use serde_json::Value;

use super::Clock;
use super::cell::RunCell;
use super::executor::{RunExecutor, RunLog};
use super::reader;

/// The most runs the registry holds at once.
pub(super) const MAX_RUNS: usize = 32;

#[derive(Default)]
struct Table {
    runs: HashMap<String, Arc<RunCell>>,
    next_order: u64,
}

impl Table {
    /// Drop every ended run whose retention ran out at `now`.
    fn sweep(&mut self, now: u64) {
        self.runs.retain(|_, cell| {
            let keep = !cell.expired(now);
            if !keep {
                cell.drop_now();
                tracing::debug!(run = %cell.id, "run dropped: retention ran out");
            }
            keep
        });
    }

    /// Make room for one more run, dropping the oldest ended run if every
    /// slot is taken.
    fn make_room(&mut self) -> Result<(), RunsError> {
        if self.runs.len() < MAX_RUNS {
            return Ok(());
        }
        let oldest = self
            .runs
            .values()
            .filter(|cell| cell.is_ended())
            .min_by_key(|cell| cell.order)
            .map(|cell| cell.id.clone())
            .ok_or(RunsError::TooManyRuns)?;
        if let Some(cell) = self.runs.remove(&oldest) {
            cell.drop_now();
            tracing::debug!(run = %cell.id, "run dropped to make room");
        }
        Ok(())
    }
}

/// What an error's message becomes for a reader outside the run's scope.
pub(super) const OTHERS_MESSAGE: &str =
    "The run failed; its details are for the device that started it.";

/// A run as `reader` may see it. An error's message can quote the reply, so
/// a reader outside the run's own scope gets the code with fixed text.
fn seen_by(cell: &RunCell, reader: &RunScope) -> RunInfo {
    let mut info = cell.info();
    if cell.scope != *reader {
        if let Some(error) = &mut info.error {
            OTHERS_MESSAGE.clone_into(&mut error.message);
        }
    }
    info
}

/// Every run this daemon holds. See the [module docs](super).
pub struct RunRegistry {
    table: Mutex<Table>,
    executor: Arc<dyn RunExecutor>,
    clock: Clock,
}

impl RunRegistry {
    pub(crate) fn new(executor: Arc<dyn RunExecutor>, clock: Clock) -> Self {
        Self {
            table: Mutex::new(Table::default()),
            executor,
            clock,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Table> {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        table.sweep((self.clock)());
        table
    }

    /// The run `scope` may see by that id.
    fn visible(&self, scope: &RunScope, id: &str) -> Result<Arc<RunCell>, RunsError> {
        let cell = self
            .lock()
            .runs
            .get(id)
            .cloned()
            .ok_or(RunsError::NotFound)?;
        if *scope == RunScope::Local || cell.scope == *scope {
            Ok(cell)
        } else {
            Err(RunsError::NotFound)
        }
    }

    /// Cancel and drop every run, ending every reader. For the daemon's
    /// shutdown.
    pub fn shutdown(&self) {
        let drained: Vec<_> = self.lock().runs.drain().map(|(_, cell)| cell).collect();
        for cell in &drained {
            cell.drop_now();
        }
        tracing::debug!(runs = drained.len(), "runs dropped at shutdown");
    }

    async fn drive(executor: Arc<dyn RunExecutor>, cell: Arc<RunCell>, body: Value) {
        let log = RunLog::new(Arc::clone(&cell));
        let outcome = tokio::select! {
            () = cell.cancel.cancelled() => None,
            outcome = executor.execute(body, log) => Some(outcome),
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
    }
}

impl RunsPort for RunRegistry {
    fn create(&self, scope: RunScope, id: &str, body: Value) -> Result<Created, RunsError> {
        if !is_run_id(id) {
            return Err(RunsError::InvalidId);
        }
        if !body.is_object() {
            return Err(RunsError::InvalidBody);
        }
        let mut table = self.lock();
        if let Some(cell) = table.runs.get(id) {
            return if cell.scope == scope {
                Ok(Created {
                    info: cell.info(),
                    created: false,
                })
            } else {
                Err(RunsError::IdTaken)
            };
        }
        table.make_room()?;
        let model = body.get("model").and_then(Value::as_str).map(str::to_owned);
        let order = table.next_order;
        table.next_order += 1;
        let cell = Arc::new(RunCell::new(
            id,
            scope,
            order,
            model,
            Arc::clone(&self.clock),
        ));
        table.runs.insert(id.to_owned(), Arc::clone(&cell));
        let held = table.runs.len();
        drop(table);
        let info = cell.info();
        tracing::debug!(run = %id, runs_held = held, "run created");
        tokio::spawn(Self::drive(Arc::clone(&self.executor), cell, body));
        Ok(Created {
            info,
            created: true,
        })
    }

    fn list(&self, scope: &RunScope) -> RunList {
        let table = self.lock();
        let mut cells: Vec<&Arc<RunCell>> = table
            .runs
            .values()
            .filter(|cell| *scope == RunScope::Local || cell.scope == *scope)
            .collect();
        cells.sort_by_key(|cell| std::cmp::Reverse(cell.order));
        let runs: Vec<RunInfo> = cells.iter().map(|cell| seen_by(cell, scope)).collect();
        drop(table);
        RunList { runs }
    }

    fn get(&self, scope: &RunScope, id: &str) -> Result<RunInfo, RunsError> {
        Ok(seen_by(&*self.visible(scope, id)?, scope))
    }

    fn events(&self, scope: &RunScope, id: &str, after: u32) -> Result<RunEvents, RunsError> {
        let cell = self.visible(scope, id)?;
        if cell.scope != *scope {
            return Err(RunsError::NotYours);
        }
        Ok(reader::events(cell, after))
    }

    fn cancel(&self, scope: &RunScope, id: &str) -> Result<RunInfo, RunsError> {
        let cell = self.visible(scope, id)?;
        cell.cancel();
        let info = seen_by(&cell, scope);
        tracing::debug!(run = %id, status = ?info.status, "run cancel asked for");
        Ok(info)
    }

    fn forget_device(&self, device: &str) -> usize {
        let scope = RunScope::Device(device.to_owned());
        let mut table = self.lock();
        let ids: Vec<String> = table
            .runs
            .values()
            .filter(|cell| cell.scope == scope)
            .map(|cell| cell.id.clone())
            .collect();
        let dropped: Vec<_> = ids.iter().filter_map(|id| table.runs.remove(id)).collect();
        drop(table);
        for cell in &dropped {
            cell.drop_now();
        }
        tracing::debug!(runs = dropped.len(), "a forgotten device's runs dropped");
        dropped.len()
    }
}
