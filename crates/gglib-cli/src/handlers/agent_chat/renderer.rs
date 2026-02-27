//! Maps [`AgentEvent`] variants to terminal output.
//!
//! The full implementation is added in the next commit.

use gglib_core::domain::agent::AgentEvent;

/// Render a single agent event to stdout/stderr.
///
/// `verbose` enables iteration-progress lines that are suppressed by default.
pub fn render_event(_event: &AgentEvent, _verbose: bool) {
    todo!("agent_chat::renderer::render_event — full implementation in Commit 3")
}
