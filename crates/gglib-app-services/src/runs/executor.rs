//! What does a run's work: the seam between the registry and the upstream.
//!
//! The registry owns a run's life; an executor only produces its events. It
//! is handed the request body and a [`RunLog`], and its future is dropped
//! when the run is cancelled or its log fills, which is what drops the
//! upstream connection.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::agent::PreviewFrame;
use gglib_core::domain::runs::RunError;
use serde::Serialize;
use serde_json::Value;

use super::cell::{RunCell, Stopped};

/// Where an executor writes a run's events.
pub struct RunLog {
    cell: Arc<RunCell>,
}

impl RunLog {
    pub(super) const fn new(cell: Arc<RunCell>) -> Self {
        Self { cell }
    }

    /// The run's id; the scripted executor finds its script by it.
    #[cfg(test)]
    pub(crate) fn id(&self) -> &str {
        &self.cell.id
    }

    /// The upstream answered with a reply: the run is `in_progress`.
    pub fn started(&self) {
        self.cell.started();
    }

    /// Log one event. An `Err` means the run has ended and the executor
    /// should stop.
    ///
    /// # Errors
    ///
    /// [`Stopped`] once the run has ended.
    pub fn append(&self, frame: String) -> Result<(), Stopped> {
        self.cell.append(frame)
    }

    /// Keep `frame` as the run's latest preview, for `tool_call_id`. It is
    /// sent to readers that have caught up with the log and is never logged:
    /// the log's bytes and `last_seq` do not change.
    pub fn preview(&self, tool_call_id: &str, frame: &PreviewFrame) {
        #[derive(Serialize)]
        struct Data<'a> {
            tool_call_id: &'a str,
            frame: &'a PreviewFrame,
        }
        let data = Data {
            tool_call_id,
            frame,
        };
        if let Ok(json) = serde_json::to_string(&data) {
            self.cell.preview(tool_call_id, json);
        }
    }

    /// Log the event that says `tool_call_id`'s call has finished, and
    /// forget the run's preview if it belongs to that call, in one step: no
    /// reader gets that call's frame after its completion. Another call's
    /// frame stays.
    ///
    /// # Errors
    ///
    /// [`Stopped`] once the run has ended.
    pub fn append_completing(&self, frame: String, tool_call_id: &str) -> Result<(), Stopped> {
        self.cell.append_completing(frame, tool_call_id)
    }
}

/// Produces one kind of run.
#[async_trait]
pub(crate) trait RunExecutor: Send + Sync {
    /// Do the run's work, logging each event. `Ok` completes the run and
    /// `Err` fails it with that error.
    async fn execute(&self, body: Value, log: RunLog) -> Result<(), RunError>;
}
