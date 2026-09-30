//! A run's event stream: the log from a cursor, then live events, then the
//! run's final state.
//!
//! Each step reads the log by index under the cell's lock and, with nothing
//! new, waits on the cell's `watch`. The version is marked seen *before* the
//! log is read, so a frame logged between the read and the wait still wakes
//! the reader.

use std::collections::VecDeque;
use std::sync::Arc;

use futures_util::stream;
use gglib_core::ports::{RunEvent, RunEvents};
use tokio::sync::watch;

use super::cell::{RunCell, Step};

struct Reader {
    cell: Arc<RunCell>,
    /// How many frames this reader has seen, which is the seq of the last.
    cursor: usize,
    changed: watch::Receiver<u64>,
    pending: VecDeque<RunEvent>,
    done: bool,
    /// Whether the reader is the run's own scope.
    owner: bool,
}

impl Reader {
    async fn next(&mut self) -> Option<RunEvent> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Some(event);
            }
            if self.done {
                return None;
            }
            self.changed.borrow_and_update();
            match self.cell.step(self.cursor) {
                Step::Frames(frames) => {
                    for data in frames {
                        self.cursor += 1;
                        let seq = u32::try_from(self.cursor).unwrap_or(u32::MAX);
                        self.pending.push_back(RunEvent::Frame { seq, data });
                    }
                }
                Step::End(info) => {
                    // The run's own scope reaching the end starts the short
                    // retention; another reader of a hub chat's run does not.
                    if self.owner {
                        self.cell.mark_read();
                    }
                    self.done = true;
                    return Some(RunEvent::End(info));
                }
                Step::Dropped => {
                    self.done = true;
                }
                Step::Wait => {
                    if self.changed.changed().await.is_err() {
                        self.done = true;
                    }
                }
            }
        }
    }
}

/// The events of `cell` after `after`, for a reader who may read them;
/// `owner` when it is the run's own scope.
pub(super) fn events(cell: Arc<RunCell>, after: u32, owner: bool) -> RunEvents {
    let reader = Reader {
        owner,
        changed: cell.subscribe(),
        cell,
        cursor: after as usize,
        pending: VecDeque::new(),
        done: false,
    };
    Box::pin(stream::unfold(reader, |mut reader| async move {
        reader.next().await.map(|event| (event, reader))
    }))
}
