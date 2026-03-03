//! Reusable predicates for asserting on [`AgentEvent`] slices.
#![allow(dead_code)]
//!
//! Import with `use common::event_assertions::*;` in integration and unit
//! test files.

use gglib_core::domain::agent::AgentEvent;
use tokio::sync::mpsc;

/// Drain all events buffered in `rx` after the sending end has been dropped.
///
/// [`AgentLoopPort::run`] takes the [`mpsc::Sender`] by value and drops it on
/// return, so by the time a test calls this helper the channel is already
/// closed — `recv()` will return `None` after the last buffered event.
///
/// # Deadlock risk
///
/// This function blocks until the channel is **closed**, i.e., until every
/// [`mpsc::Sender`] clone has been dropped.  If any sender clone is still
/// alive when this is called — for example, a background task that holds a
/// cloned `tx` but has not yet been awaited or aborted — `recv()` will block
/// forever and the test will hang.
///
/// **Always** call this *after* `await`ing the agent run (which consumes and
/// drops the `tx` passed to [`AgentLoopPort::run`]) and after joining any
/// other tasks that hold `tx` clones.
pub async fn collect_events(mut rx: mpsc::Receiver<AgentEvent>) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Some(evt) = rx.recv().await {
        events.push(evt);
    }
    events
}

/// Return `true` when `events` contains at least one [`AgentEvent::FinalAnswer`].
pub fn has_final_answer(events: &[AgentEvent]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, AgentEvent::FinalAnswer { .. }))
}

/// Return `true` when `events` contains at least one
/// [`AgentEvent::ToolCallStart`] with the given tool name.
pub fn has_tool_start(events: &[AgentEvent], name: &str) -> bool {
    events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolCallStart { tool_call, .. } if tool_call.name == name))
}

/// Return `true` when `events` contains at least one
/// [`AgentEvent::ToolCallComplete`] whose result has the given `success` value.
pub fn has_tool_complete_with_success(events: &[AgentEvent], success: bool) -> bool {
    events.iter().any(
        |e| matches!(e, AgentEvent::ToolCallComplete { result, .. } if result.success == success),
    )
}

/// Return `true` when `events` contains at least one [`AgentEvent::Error`].
pub fn has_error_event(events: &[AgentEvent]) -> bool {
    events.iter().any(|e| matches!(e, AgentEvent::Error { .. }))
}
