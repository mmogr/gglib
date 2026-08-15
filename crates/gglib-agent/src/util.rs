//! Shared async helpers for the `gglib-agent` crate.

use gglib_core::AgentEvent;
use tokio::sync::mpsc;

/// Emit an [`AgentEvent::Error`] on `tx`, ignoring send failures.
///
/// Called before every early-return that propagates an error out of the agent
/// loop, so that SSE / CLI consumers always receive an `error` event before
/// the channel closes — regardless of which module detected the failure.
pub(crate) async fn emit_error_event(tx: &mpsc::Sender<AgentEvent>, message: &str) {
    let _ = tx
        .send(AgentEvent::Error {
            message: message.to_owned(),
        })
        .await;
}
