//! Stateful streaming accumulator for thinking-tag detection.
//!
//! An LLM may stream `<` in one SSE chunk, `thi` in the next, and `nk>` in a
//! third.  [`ThinkingAccumulator`] handles this by buffering partial tags
//! and only emitting classified [`ThinkingEvent`]s once enough bytes have
//! arrived to decide.

use std::fmt;

use super::normalize::normalize_thinking_tags;
use super::types::ThinkingEvent;

/// Internal state of the accumulator FSM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccState {
    /// Haven't seen the opening `<think…>` yet (may be buffering a partial tag).
    AwaitingOpen,
    /// Inside the thinking block, forwarding as `ThinkingDelta`.
    InsideThinking,
    /// Past the closing `</think>`, forwarding as `ContentDelta`.
    ContentPhase,
}

/// Stateful accumulator for streaming thinking-tag detection.
///
/// Feed SSE text deltas one-by-one via [`push`](Self::push) and collect the
/// returned [`ThinkingEvent`]s.  The accumulator handles tags that are split
/// arbitrarily across chunks.
///
/// # Example
///
/// ```
/// use gglib_core::domain::thinking::{ThinkingAccumulator, ThinkingEvent};
///
/// let mut acc = ThinkingAccumulator::new();
///
/// // Tag arrives split across three chunks
/// let e1 = acc.push("<thi");
/// assert!(e1.is_empty()); // buffered
///
/// let e2 = acc.push("nk>");
/// assert_eq!(e2, vec![]); // open tag consumed, no content yet
///
/// let e3 = acc.push("hmm");
/// assert_eq!(e3, vec![ThinkingEvent::ThinkingDelta("hmm".into())]);
///
/// let e4 = acc.push("</think>");
/// assert_eq!(e4, vec![ThinkingEvent::ThinkingEnd]);
///
/// let e5 = acc.push("Hello!");
/// assert_eq!(e5, vec![ThinkingEvent::ContentDelta("Hello!".into())]);
/// ```
pub struct ThinkingAccumulator {
    state: AccState,
    /// Buffer for bytes that *might* be part of an opening or closing tag.
    buf: String,
}

impl fmt::Debug for ThinkingAccumulator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThinkingAccumulator")
            .field("state", &self.state)
            .field("buf_len", &self.buf.len())
            .finish()
    }
}

/// All possible opening-tag prefixes after normalisation (lowercase).
/// Used to determine whether the buffer *could* still become a valid tag.
const OPEN_TAG_PREFIX: &str = "<think";

/// The canonical closing tag (lowercase for matching).
const CLOSE_TAG: &str = "</think>";

impl ThinkingAccumulator {
    /// Create a new accumulator in the initial state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: AccState::AwaitingOpen,
            buf: String::new(),
        }
    }

    /// Feed a new text chunk and return any events produced.
    pub fn push(&mut self, chunk: &str) -> Vec<ThinkingEvent> {
        if chunk.is_empty() {
            return vec![];
        }
        match self.state {
            AccState::AwaitingOpen => self.handle_awaiting_open(chunk),
            AccState::InsideThinking => self.handle_inside_thinking(chunk),
            AccState::ContentPhase => Self::handle_content(chunk),
        }
    }

    /// Flush any remaining buffered content.
    ///
    /// Call this when the stream ends to emit any buffered text that turned
    /// out not to be a tag.
    pub fn flush(&mut self) -> Vec<ThinkingEvent> {
        if self.buf.is_empty() {
            return vec![];
        }

        let text = std::mem::take(&mut self.buf);
        match self.state {
            AccState::AwaitingOpen => {
                // Never completed an open tag — treat as plain content.
                self.state = AccState::ContentPhase;
                vec![ThinkingEvent::ContentDelta(text)]
            }
            AccState::InsideThinking => {
                // Stream ended mid-thinking — flush as thinking.
                vec![ThinkingEvent::ThinkingDelta(text)]
            }
            AccState::ContentPhase => {
                vec![ThinkingEvent::ContentDelta(text)]
            }
        }
    }

    // -- Private state handlers -----------------------------------------------

    fn handle_awaiting_open(&mut self, chunk: &str) -> Vec<ThinkingEvent> {
        // Normalise variant tags in the incoming chunk.
        let normalized = normalize_thinking_tags(chunk);
        self.buf.push_str(&normalized);

        let lower = self.buf.to_lowercase();

        // 1. Check if buffer contains a complete open tag (with closing '>').
        if let Some(gt_pos) = Self::find_open_tag_end(&lower) {
            // We have a complete open tag.  Anything before `<think` is content.
            let lt_pos = lower.find(OPEN_TAG_PREFIX).unwrap_or(0);
            let mut events = vec![];

            if lt_pos > 0 {
                let before = self.buf[..lt_pos].to_string();
                if !before.is_empty() {
                    // Text before thinking tag goes to content.
                    self.state = AccState::ContentPhase;
                    events.push(ThinkingEvent::ContentDelta(before));
                    // Actually, if there's content before <think>, the thinking
                    // tag is not at the start — treat entire thing as content.
                    let rest = self.buf[lt_pos..].to_string();
                    events.push(ThinkingEvent::ContentDelta(rest));
                    self.buf.clear();
                    return events;
                }
            }

            // Consume the open tag and move to InsideThinking.
            let after = self.buf[gt_pos + 1..].to_string();
            self.buf.clear();
            self.state = AccState::InsideThinking;

            // Any text after the '>' is the first thinking chunk.
            if !after.is_empty() {
                events.extend(self.handle_inside_thinking(&after));
            }

            return events;
        }

        // 2. Check if buffer could still be a prefix of `<think...>`.
        if is_potential_open_tag_prefix(&lower) {
            // Keep buffering.
            return vec![];
        }

        // 3. Not a thinking tag — flush buffer as content.
        let text = std::mem::take(&mut self.buf);
        self.state = AccState::ContentPhase;
        vec![ThinkingEvent::ContentDelta(text)]
    }

    fn handle_inside_thinking(&mut self, chunk: &str) -> Vec<ThinkingEvent> {
        // Normalise variant close tags (e.g. </reasoning> → </think>).
        let normalized = normalize_thinking_tags(chunk);
        self.buf.push_str(&normalized);

        let mut events = vec![];

        let lower = self.buf.to_lowercase();

        // Look for </think> in the buffer.
        if let Some(pos) = lower.find(CLOSE_TAG) {
            // Everything before the close tag is thinking content.
            let thinking = self.buf[..pos].to_string();
            let after = self.buf[pos + CLOSE_TAG.len()..].to_string();
            self.buf.clear();

            if !thinking.is_empty() {
                events.push(ThinkingEvent::ThinkingDelta(thinking));
            }
            events.push(ThinkingEvent::ThinkingEnd);

            self.state = AccState::ContentPhase;
            if !after.is_empty() {
                events.push(ThinkingEvent::ContentDelta(after));
            }
            return events;
        }

        // Check if buffer ends with a potential partial `</think>` tag.
        // e.g. buffer ends with `</thi` — we can't emit that yet.
        let safe_emit_len = safe_thinking_emit_len(&self.buf);

        if safe_emit_len > 0 {
            let to_emit = self.buf[..safe_emit_len].to_string();
            let remainder = self.buf[safe_emit_len..].to_string();
            self.buf = remainder;
            events.push(ThinkingEvent::ThinkingDelta(to_emit));
        }

        events
    }

    fn handle_content(chunk: &str) -> Vec<ThinkingEvent> {
        if chunk.is_empty() {
            return vec![];
        }
        vec![ThinkingEvent::ContentDelta(chunk.to_string())]
    }

    /// Find the position of `>` that closes an opening `<think...>` tag.
    ///
    /// Returns the byte index of `>` in `lower`.
    fn find_open_tag_end(lower: &str) -> Option<usize> {
        if !lower.starts_with(OPEN_TAG_PREFIX) {
            return None;
        }
        // Find first '>' after "<think"
        lower[OPEN_TAG_PREFIX.len()..]
            .find('>')
            .map(|p| OPEN_TAG_PREFIX.len() + p)
    }
}

impl Default for ThinkingAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Check whether `s` is a valid prefix of any opening tag we recognise.
///
/// This includes the full `<think...>` pattern as well as just `<`, `<t`,
/// `<th`, etc.  Used to decide whether to keep buffering.
fn is_potential_open_tag_prefix(s: &str) -> bool {
    let s = s.trim_start();
    if s.is_empty() {
        return false;
    }

    // Check if s is a prefix of "<think" (possibly followed by attrs + ">")
    let tag = OPEN_TAG_PREFIX;
    if s.len() <= tag.len() {
        return tag.starts_with(s);
    }

    // s is longer than "<think" — it must start with "<think" and be
    // waiting for the closing '>'.
    if !s.starts_with(tag) {
        return false;
    }

    // After "<think" we allow optional attributes until '>'.
    !s[tag.len()..].contains('>')
}

/// Return how many bytes from the start of `buf` are safe to emit as
/// thinking content — i.e. bytes that cannot be part of a `</think>` tag.
fn safe_thinking_emit_len(buf: &str) -> usize {
    let lower = buf.to_lowercase();
    // Find the earliest position where a potential closing tag could start.
    // A closing tag starts with '<'.  We need to check if '<' at position i
    // could be the start of "</think>".
    for (i, _) in lower.char_indices().rev() {
        if lower[i..].starts_with('<') {
            let tail = &lower[i..];
            // Check if tail is a prefix of "</think>"
            if CLOSE_TAG.starts_with(tail) {
                // This '<' might be the start of a closing tag.
                return i;
            }
        }
    }
    // No potential closing tag fragment found — entire buffer is safe.
    buf.len()
}
