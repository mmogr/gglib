//! Reusable predicates for asserting on [`AgentEvent`] slices.
//!
//! Import with `use common::event_assertions::*;` in integration and unit
//! test files.

use gglib_core::domain::agent::AgentEvent;

/// Return `true` when `events` contains at least one [`AgentEvent::FinalAnswer`].
pub fn has_final_answer(events: &[AgentEvent]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, AgentEvent::FinalAnswer { .. }))
}

/// Return `true` when `events` contains at least one
/// [`AgentEvent::ToolCallStart`] with the given tool name.
pub fn has_tool_start(events: &[AgentEvent], name: &str) -> bool {
    events.iter().any(
        |e| matches!(e, AgentEvent::ToolCallStart { tool_call, .. } if tool_call.name == name),
    )
}

/// Return `true` when `events` contains at least one
/// [`AgentEvent::ToolCallComplete`] whose result has the given `success` value.
pub fn has_tool_complete_with_success(events: &[AgentEvent], success: bool) -> bool {
    events.iter().any(
        |e| matches!(e, AgentEvent::ToolCallComplete { result, .. } if result.success == success),
    )
}
