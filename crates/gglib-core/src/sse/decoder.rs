//! Stateful SSE byte-stream decoder.
//!
//! [`SseStreamDecoder`] hands raw bytes from an HTTP response to
//! [`super::frames::Lines`], decodes each complete `data:` line, and delegates
//! frame parsing to [`super::parser`]. A chunk may end anywhere, inside a
//! character included: only a whole line is ever decoded. Its explicit state
//! makes it straightforward to unit-test without standing up an actual HTTP
//! server or wrapping everything in an `async_stream` macro block.

use anyhow::Result;
use tracing::debug;

use crate::LlmStreamEvent;

use super::frames::Lines;
use super::parser::{SseParseResult, parse_sse_frame};

/// Stateful decoder that turns a sequence of raw SSE byte chunks into a
/// sequence of [`LlmStreamEvent`] values.
///
/// # Usage
///
/// ```text
/// let mut decoder = SseStreamDecoder::default();
/// while let Some(chunk) = byte_stream.next().await {
///     let (events, stop) = decoder.feed_bytes(&chunk);
///     for event in events { … }
///     if stop { break; }
/// }
/// if let Some(fallback) = decoder.finish() { … }
/// ```
#[derive(Default)]
pub struct SseStreamDecoder {
    lines: Lines,
    /// Set to `true` once a [`LlmStreamEvent::Done`] has been yielded, so the
    /// `[DONE]` sentinel doesn't generate a duplicate.
    done_sent: bool,
}

impl SseStreamDecoder {
    /// Feed one raw byte chunk into the decoder.
    ///
    /// Returns `(events, should_stop)`.
    ///
    /// - `events` — zero or more parsed [`LlmStreamEvent`] values (or stream
    ///   errors) extracted from the bytes fed so far.
    /// - `should_stop` — `true` when the SSE stream has reached its natural end
    ///   (a `[DONE]` sentinel or an unrecoverable parse error).  The caller
    ///   must not feed any further chunks once this flag is `true`.
    ///
    /// A complete line that is not UTF-8 is such an error: its bytes are
    /// never replaced with a guess.
    pub fn feed_bytes(&mut self, bytes: &[u8]) -> (Vec<Result<LlmStreamEvent>>, bool) {
        self.lines.push(bytes);
        let mut events = Vec::new();

        while let Some(line) = self.lines.next_line() {
            let line = match String::from_utf8(line) {
                Ok(line) => line,
                Err(e) => {
                    events.push(Err(anyhow::anyhow!("invalid UTF-8 in LLM SSE stream: {e}")));
                    return (events, true);
                }
            };

            // Skip blank lines and SSE comment lines.
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };

            match parse_sse_frame(data) {
                Ok(SseParseResult::Done) => {
                    if !self.done_sent {
                        debug!(
                            "LLM stream ended with [DONE] but no prior finish_reason \
                             — emitting fallback Done with an unknown reason"
                        );
                        // Deliberately not "stop": the upstream never said the
                        // turn finished cleanly, and claiming it did would
                        // relabel a truncation as a complete answer.
                        events.push(Ok(LlmStreamEvent::Done {
                            finish_reason: None,
                        }));
                    }
                    self.done_sent = true;
                    return (events, true);
                }
                Ok(SseParseResult::Events(parsed_events)) => {
                    let mut saw_terminal_error = false;
                    for event in parsed_events {
                        if matches!(event, LlmStreamEvent::Done { .. }) {
                            self.done_sent = true;
                        }
                        if matches!(event, LlmStreamEvent::UpstreamError { .. }) {
                            // Terminal condition: the encoder appends its own
                            // `[DONE]` sentinel right after this event (see
                            // `SseEncoder::encode`), and nothing meaningful
                            // is expected to follow an inline upstream
                            // error. Stop feeding further bytes, same as the
                            // literal `[DONE]` sentinel case above, so a
                            // stray fallback `Done` isn't appended by
                            // `finish()`.
                            saw_terminal_error = true;
                            self.done_sent = true;
                        }
                        events.push(Ok(event));
                    }
                    if saw_terminal_error {
                        return (events, true);
                    }
                }
                Err(e) => {
                    events.push(Err(e));
                    return (events, true);
                }
            }
        }

        (events, false)
    }

    /// Emit a fallback `Done` event if the byte stream ended without one.
    ///
    /// Call this once after the upstream byte stream is fully exhausted.
    /// Returns `None` if a `Done` was already yielded by [`Self::feed_bytes`].
    #[must_use]
    pub fn finish(self) -> Option<LlmStreamEvent> {
        if self.done_sent {
            None
        } else {
            debug!(
                "LLM byte-stream ended without [DONE] sentinel — emitting fallback Done \
                 with an unknown reason"
            );
            // A byte stream that simply stopped is the strongest case for not
            // fabricating: nothing upstream ever claimed the turn was over.
            Some(LlmStreamEvent::Done {
                finish_reason: None,
            })
        }
    }
}

#[cfg(test)]
#[path = "decoder_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "decoder_chunk_tests.rs"]
mod chunk_tests;
