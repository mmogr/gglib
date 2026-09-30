//! The registry: every run this daemon holds, the scope rule, the limits and
//! retention.
//!
//! The scope rule: this machine sees every run, a device sees its own, and
//! a run on one of the hub's chats (one with a `conversation_id`) belongs to
//! the chat, so this machine and every paired device may read and cancel it.
//!
//! Shaped like the download manager: a map under a mutex, a cancellation
//! token per job. Retention is swept at the start of every call, against the
//! registry's clock, so a run is gone by the first call after its time is up.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gglib_core::domain::runs::{RunInfo, RunKind, RunList, is_run_id};
use gglib_core::ports::{Created, RunEvents, RunScope, RunsError, RunsPort};
use serde_json::Value;
use tokio::sync::watch;

use super::Clock;
use super::admit::Admitted;
use super::cell::{RunCell, RunSpec};
use super::executor::{RunExecutor, RunLog};
use super::reader;

/// The most runs the registry holds at once.
pub(super) const MAX_RUNS: usize = 32;

#[derive(Default)]
pub(super) struct Table {
    pub(super) runs: HashMap<String, Arc<RunCell>>,
    pub(super) next_order: u64,
    /// Set by `shutdown`; no run starts after it.
    pub(super) closed: bool,
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

    /// The run whose reply to `conversation_id` is not yet saved, if any.
    pub(super) fn live_on(&self, conversation_id: i64) -> Option<&Arc<RunCell>> {
        self.runs
            .values()
            .find(|cell| !cell.is_ended() && cell.info().conversation_id == Some(conversation_id))
    }

    /// Make room for one more run, dropping the oldest ended run if every
    /// slot is taken.
    pub(super) fn make_room(&mut self) -> Result<(), RunsError> {
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

/// Whether the run answers one of the hub's chats, and so belongs to it.
fn on_chat(cell: &RunCell) -> bool {
    cell.info().conversation_id.is_some()
}

/// Whether `reader` may read `cell`'s reply: its own scope, or anyone for a
/// run on a hub chat.
fn may_read(cell: &RunCell, reader: &RunScope) -> bool {
    cell.scope == *reader || on_chat(cell)
}

/// A run as `reader` may see it. An error's message can quote the reply, so
/// a reader who may not read the reply gets the code with fixed text.
fn seen_by(cell: &RunCell, reader: &RunScope) -> RunInfo {
    let mut info = cell.info();
    if !may_read(cell, reader) {
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
    pub(super) clock: Clock,
    /// How many runs are still being driven, their end included.
    pub(super) live: watch::Sender<usize>,
}

impl std::fmt::Debug for RunRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunRegistry").finish_non_exhaustive()
    }
}

impl RunRegistry {
    pub(crate) fn new(executor: Arc<dyn RunExecutor>, clock: Clock) -> Self {
        Self {
            table: Mutex::new(Table::default()),
            executor,
            clock,
            live: watch::Sender::new(0),
        }
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, Table> {
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
        if *scope == RunScope::Local || may_read(&cell, scope) {
            Ok(cell)
        } else {
            Err(RunsError::NotFound)
        }
    }

    /// The run whose reply to `conversation_id` is not yet saved, by id:
    /// the run that would refuse another turn to it.
    #[must_use]
    pub fn live_on(&self, conversation_id: i64) -> Option<String> {
        self.lock()
            .live_on(conversation_id)
            .map(|cell| cell.id.clone())
    }

    /// Cancel and drop every run, ending every reader. For the daemon's
    /// shutdown.
    pub fn shutdown(&self) {
        let drained: Vec<_> = {
            let mut table = self.lock();
            table.closed = true;
            table.runs.drain().map(|(_, cell)| cell).collect()
        };
        for cell in &drained {
            cell.drop_now();
        }
        tracing::debug!(runs = drained.len(), "runs dropped at shutdown");
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
        // Always a chat run: an agent run is started only through the
        // daemon's own routes, never through this port.
        let spec = RunSpec {
            kind: RunKind::Chat,
            model: body.get("model").and_then(Value::as_str).map(str::to_owned),
            conversation_id: None,
        };
        // No end handler: a chat run is settled the moment it ends.
        let cell = match self.admit(scope, id, spec, false)? {
            Admitted::Existing(info) => {
                return Ok(Created {
                    info,
                    created: false,
                });
            }
            Admitted::New(cell) => cell,
        };
        let info = cell.info();
        let executor = Arc::clone(&self.executor);
        let log = RunLog::new(Arc::clone(&cell));
        self.spawn(
            cell,
            Box::pin(async move { executor.execute(body, log).await }),
            None,
        );
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
            .filter(|cell| *scope == RunScope::Local || may_read(cell, scope))
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
        if !may_read(&cell, scope) {
            return Err(RunsError::NotYours);
        }
        let owner = cell.scope == *scope;
        Ok(reader::events(cell, after, owner))
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
            // A run on a hub chat is the chat's: it goes on, and its reply
            // is saved.
            .filter(|cell| cell.scope == scope && !on_chat(cell))
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
