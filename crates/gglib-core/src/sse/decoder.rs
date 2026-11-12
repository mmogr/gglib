//! Stateful SSE byte-stream decoder.
//!
//! [`SseStreamDecoder`] hands raw bytes from an HTTP response to
//! [`super::frames::Lines`] and reads each complete line. A chunk may end
//! anywhere, inside a character included: only a whole line is ever read.
//! [`SseStreamDecoder::feed_frames`] gives each line as an [`SseItem`]: a
//! comment, a `data:` frame with its JSON and its events (from
//! [`super::parser`]), or `[DONE]`. [`SseStreamDecoder::feed_bytes`] gives the
//! events alone. Its explicit state makes it straightforward to unit-test
//! without standing up an actual HTTP server or wrapping everything in an
//! `async_stream` macro block.

use anyhow::Result;
use serde_json::Value;
use tracing::debug;

use crate::LlmStreamEvent;

use super::frames::Lines;
use super::parser::{events_of, frame_json};

/// One line of an SSE stream that says something, as
/// [`SseStreamDecoder::feed_frames`] reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseItem {
    /// A comment line, its leading `:` included, such as llama-server's
    /// keep-alive `:`.
    Comment(String),
    /// A `data:` frame other than `[DONE]`.
    Data {
        /// The payload after `data: `, as it arrived.
        text: String,
        /// The payload parsed.
        value: Value,
        /// The events it carries ([`events_of`]); none for a frame such as
        /// llama-server's opening role frame.
        events: Vec<LlmStreamEvent>,
    },
    /// The `[DONE]` sentinel.
    Done,
}

/// Stateful decoder that turns a sequence of raw SSE byte chunks into a
/// sequence of [`SseItem`]s, or of [`LlmStreamEvent`]s.
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
    /// Set to `true` once [`Self::feed_bytes`] has yielded a
    /// [`LlmStreamEvent::Done`], so the `[DONE]` sentinel doesn't generate a
    /// duplicate.
    done_sent: bool,
}

impl SseStreamDecoder {
    /// Feed one raw byte chunk into the decoder, and read every complete line
    /// it finishes.
    ///
    /// Returns `(items, should_stop)`. Blank lines and fields other than
    /// `data:` give no item. `should_stop` is `true` once the stream has
    /// ended: at `[DONE]`, after a frame that carries an
    /// [`LlmStreamEvent::UpstreamError`] (nothing meaningful follows one), or
    /// at a line that is not UTF-8 or a payload that is not JSON, each of
    /// which is an `Err` item and is never replaced with a guess. The caller
    /// must not feed any further chunks once it is `true`.
    pub fn feed_frames(&mut self, bytes: &[u8]) -> (Vec<Result<SseItem>>, bool) {
        self.lines.push(bytes);
        let mut items = Vec::new();

        while let Some(line) = self.lines.next_line() {
            let line = match String::from_utf8(line) {
                Ok(line) => line,
                Err(e) => {
                    items.push(Err(anyhow::anyhow!("invalid UTF-8 in LLM SSE stream: {e}")));
                    return (items, true);
                }
            };
            if line.starts_with(':') {
                items.push(Ok(SseItem::Comment(line)));
                continue;
            }
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            if data == "[DONE]" {
                items.push(Ok(SseItem::Done));
                return (items, true);
            }
            let value = match frame_json(data) {
                Ok(value) => value,
                Err(e) => {
                    items.push(Err(e));
                    return (items, true);
                }
            };
            let events = events_of(&value);
            let terminal = events
                .iter()
                .any(|event| matches!(event, LlmStreamEvent::UpstreamError { .. }));
            items.push(Ok(SseItem::Data {
                text: data.to_owned(),
                value,
                events,
            }));
            if terminal {
                return (items, true);
            }
        }

        (items, false)
    }

    /// Feed one raw byte chunk into the decoder: [`Self::feed_frames`], with
    /// only the events kept.
    ///
    /// Returns `(events, should_stop)`.
    ///
    /// - `events` — zero or more parsed [`LlmStreamEvent`] values (or stream
    ///   errors) extracted from the bytes fed so far.
    /// - `should_stop` — `true` when the SSE stream has reached its natural end
    ///   (a `[DONE]` sentinel or an unrecoverable parse error).  The caller
    ///   must not feed any further chunks once this flag is `true`.
    ///
    /// A `[DONE]` with no finish reason before it gives a fallback `Done` that
    /// carries none.
    pub fn feed_bytes(&mut self, bytes: &[u8]) -> (Vec<Result<LlmStreamEvent>>, bool) {
        let (items, stop) = self.feed_frames(bytes);
        let mut events = Vec::new();
        for item in items {
            match item {
                Err(e) => events.push(Err(e)),
                Ok(SseItem::Comment(_)) => {}
                Ok(SseItem::Done) => {
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
                }
                Ok(SseItem::Data { events: parsed, .. }) => {
                    for event in parsed {
                        // An inline upstream error ends the turn as a `Done`
                        // does, so `finish()` appends no fallback after it.
                        if matches!(
                            event,
                            LlmStreamEvent::Done { .. } | LlmStreamEvent::UpstreamError { .. }
                        ) {
                            self.done_sent = true;
                        }
                        events.push(Ok(event));
                    }
                }
            }
        }
        (events, stop)
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

#[cfg(test)]
#[path = "decoder_frames_tests.rs"]
mod frames_tests;
