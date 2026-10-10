//! [`RateLimitedSink`]: a running tool's progress as agent events, no more
//! than a person can read.
//!
//! A render steps every few seconds and could report many times a step; the
//! log a run keeps is capped, and every logged event is replayed on each
//! reconnect. So a stage change, a new pass or the last step of a pass goes
//! out at once, and anything else at most once a [`PROGRESS_INTERVAL`]. A
//! preview frame always goes out, as a `ToolPreview` that is never logged.
//! Nothing here awaits: a full channel drops the event rather than slowing
//! the tool.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use gglib_core::AgentEvent;
use gglib_core::domain::agent::{ToolProgressSink, ToolProgressUpdate, ToolStage};
use tokio::sync::mpsc;

/// The most often a tool's progress is sent when nothing but its step moved.
pub(super) const PROGRESS_INTERVAL: Duration = Duration::from_secs(1);

/// What the time is. An argument, so a test can move it.
pub(super) type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// The last progress sent, to judge the next against.
struct Sent {
    stage: ToolStage,
    pass: Option<u32>,
    done: Option<u32>,
    at: Instant,
}

/// Turns one tool call's [`ToolProgressUpdate`]s into `ToolProgress` and
/// `ToolPreview` events on the loop's channel.
pub(super) struct RateLimitedSink {
    tool_call_id: String,
    tx: mpsc::Sender<AgentEvent>,
    clock: Clock,
    last: Mutex<Option<Sent>>,
}

impl RateLimitedSink {
    pub(super) fn new(tool_call_id: String, tx: mpsc::Sender<AgentEvent>, clock: Clock) -> Self {
        Self {
            tool_call_id,
            tx,
            clock,
            last: Mutex::new(None),
        }
    }

    /// Whether `update` goes out now, recording it as sent when it does.
    fn due(&self, update: &ToolProgressUpdate) -> bool {
        let now = (self.clock)();
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        let due = last.as_ref().is_none_or(|sent| {
            let last_step = matches!((update.done, update.total), (Some(d), Some(t)) if d == t);
            sent.stage != update.stage
                || sent.pass != update.pass
                || (last_step && sent.done != update.done)
                || now.saturating_duration_since(sent.at) >= PROGRESS_INTERVAL
        });
        if due {
            *last = Some(Sent {
                stage: update.stage,
                pass: update.pass,
                done: update.done,
                at: now,
            });
        }
        due
    }
}

impl ToolProgressSink for RateLimitedSink {
    fn progress(&self, update: ToolProgressUpdate) {
        if self.due(&update) {
            let _ = self.tx.try_send(AgentEvent::ToolProgress {
                tool_call_id: self.tool_call_id.clone(),
                stage: update.stage,
                pass: update.pass,
                done: update.done,
                total: update.total,
                position: update.position,
            });
        }
        if let Some(frame) = update.preview {
            let _ = self.tx.try_send(AgentEvent::ToolPreview {
                tool_call_id: self.tool_call_id.clone(),
                frame,
            });
        }
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
