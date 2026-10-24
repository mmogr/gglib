//! The `tool_call_writing` events a reply sends while its tool calls stream.
//!
//! The collector keeps a call's pieces until the reply ends, so without
//! these a reply spent writing one shows nothing at all: no text, no
//! thinking, only a wait. The first piece that names a tool is reported at
//! once, so a person sees which tool the model is calling; after that at
//! most one report a second, which a run's log can carry for as long as a
//! turn lasts.
//!
//! The pieces are the stream's `ToolCallDelta`s. Where llama-server streams
//! the call itself, that is about one a token. A model whose calls come as
//! text markup (a dialect spec) has the markup held back by the parser and
//! released as one delta once the call is complete, so its call is reported
//! once, at the end, with a small count.

use std::time::Duration;

use gglib_core::AgentEvent;
use gglib_core::domain::agent::ToolCallWriting;
use gglib_core::domain::agent::tool_display::{format_tool_display_name, strip_tool_prefix};
use tokio::time::Instant;

/// The least time between two reports of one reply.
pub(crate) const REPORT_EVERY: Duration = Duration::from_secs(1);

/// One reply's reports: the pieces its calls have streamed, and when the
/// last report was sent.
#[derive(Debug, Default)]
pub(crate) struct CallWritingReports {
    pieces: u32,
    last: Option<Instant>,
}

impl CallWritingReports {
    /// One more piece of a call arrived at `now`, the call naming `tool`
    /// once some piece of it has: the event to send for it, when one is due.
    /// A piece of a call no piece has named yet is counted, not reported.
    pub(crate) fn piece(&mut self, tool: Option<&str>, now: Instant) -> Option<AgentEvent> {
        self.pieces = self.pieces.saturating_add(1);
        let tool = tool?;
        if self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) < REPORT_EVERY)
        {
            return None;
        }
        self.last = Some(now);
        Some(AgentEvent::ToolCallWriting(ToolCallWriting {
            tool_name: tool.to_owned(),
            display_name: format_tool_display_name(strip_tool_prefix(tool)),
            tokens: self.pieces,
        }))
    }
}

#[cfg(test)]
#[path = "call_writing_tests.rs"]
mod tests;
