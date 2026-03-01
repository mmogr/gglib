//! Common test infrastructure for `gglib-agent` integration tests.
//!
//! - [`mock_llm`] — configurable [`LlmCompletionPort`] that serves scripted
//!   responses without any HTTP server.
//! - [`mock_tools`] — configurable [`ToolExecutorPort`] with per-tool
//!   behaviour (instant, delayed, fail, infra-error) and call recording.
//! - [`collect_events`] — drain an `mpsc::Receiver<AgentEvent>` to a `Vec`.

pub mod mock_llm;
pub mod mock_tools;

use gglib_core::domain::agent::AgentEvent;
use tokio::sync::mpsc;

/// Drain all events buffered in `rx` after the sending end has been dropped.
///
/// [`AgentLoopPort::run`] takes the [`mpsc::Sender`] by value and drops it on
/// return, so by the time a test calls this helper the channel is already
/// closed — `recv()` will return `None` after the last buffered event.
pub async fn collect_events(mut rx: mpsc::Receiver<AgentEvent>) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Some(evt) = rx.recv().await {
        events.push(evt);
    }
    events
}
