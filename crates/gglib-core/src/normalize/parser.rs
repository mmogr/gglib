//! The [`ToolCallParser`] trait and its companion [`ParserOutput`].
//!
//! A parser consumes raw text or reasoning chunks from an LLM stream and
//! produces a normalized [`ParserOutput`] containing:
//!
//! - `forward_text` — bytes that should appear in the downstream
//!   `LlmStreamEvent::TextDelta`.
//! - `forward_reasoning` — bytes that should appear in the downstream
//!   `LlmStreamEvent::ReasoningDelta`.
//! - `tool_calls` — fully-assembled tool calls extracted from dialect
//!   markup (e.g. Qwen XML).
//! - `errors` — non-fatal normalization failures.
//!
//! Parsers are stream-stateful: the caller (the `NormalizingStream` adapter)
//! constructs one parser per stream and drives it with chunk-by-chunk input.
//! Parsers must therefore be chunk-safe — they buffer ambiguous trailing
//! bytes internally and flush them on the next call or at [`ToolCallParser::finish`].
//!
//! ## Adding a new parser
//!
//! 1. Implement [`ToolCallParser`] in a new file under
//!    [`super::parsers`].
//! 2. Add a single match arm to [`super::registry::get_parser`] keyed on a
//!    new `format:*` tag (see [`super::tags`]).
//!
//! No other crate participates in the dispatch decision.

use super::error::NormalizationError;
use crate::domain::agent::ToolCall;

/// Result of feeding one chunk of input to a parser.
///
/// All four fields are independent: a single chunk can produce text bytes,
/// reasoning bytes, completed tool calls, and errors all at once.  Empty
/// vectors / strings are the common case and indicate "nothing to flush".
#[derive(Debug, Default, Clone)]
pub struct ParserOutput {
    /// Bytes to emit on the downstream text channel.
    pub forward_text: String,
    /// Bytes to emit on the downstream reasoning channel.
    pub forward_reasoning: String,
    /// Tool calls fully assembled by this chunk.  Each item is ready to be
    /// emitted as a single, complete `LlmStreamEvent::ToolCallDelta`.
    pub tool_calls: Vec<ToolCall>,
    /// Non-fatal normalization issues detected by this chunk.
    pub errors: Vec<NormalizationError>,
}

impl ParserOutput {
    /// Convenience constructor for a passthrough text chunk.
    #[must_use]
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            forward_text: s.into(),
            ..Self::default()
        }
    }

    /// Convenience constructor for a passthrough reasoning chunk.
    #[must_use]
    pub fn reasoning(s: impl Into<String>) -> Self {
        Self {
            forward_reasoning: s.into(),
            ..Self::default()
        }
    }

    /// `true` when this output carries no bytes, no tool calls, and no errors.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.forward_text.is_empty()
            && self.forward_reasoning.is_empty()
            && self.tool_calls.is_empty()
            && self.errors.is_empty()
    }
}

/// Stream-stateful parser that normalizes a single LLM dialect into
/// canonical [`ParserOutput`] fragments.
///
/// Implementations MUST:
///
/// - Be chunk-safe: dialect markers may straddle chunk boundaries, so the
///   parser must internally buffer ambiguous trailing bytes and flush them
///   on a later call or at [`Self::finish`].
/// - Be deterministic: feeding the same byte sequence in any chunking yields
///   the same total output (modulo when individual bytes flush).
/// - Never lose input bytes: every byte of input is either forwarded
///   verbatim, consumed as part of a recognised marker, or surfaced via a
///   [`NormalizationError`].
///
/// Implementations are NOT required to be `Send` here — the
/// `NormalizingStream` adapter erases the parser through `Box<dyn …>` and
/// adds the `Send` bound at that boundary.
pub trait ToolCallParser: Send {
    /// Feed a chunk that arrived on the upstream text channel.
    fn push_text(&mut self, chunk: &str) -> ParserOutput;

    /// Feed a chunk that arrived on the upstream reasoning channel.
    fn push_reasoning(&mut self, chunk: &str) -> ParserOutput;

    /// Flush any buffered partial state at end-of-stream.
    ///
    /// Called exactly once per stream, after the last `push_*` call and
    /// before the surrounding `Done` event is forwarded downstream.
    /// Implementations should emit any held-back bytes as text and surface
    /// any unfinished marker state as an [`NormalizationError`].
    fn finish(&mut self) -> ParserOutput;
}
