//! Stateful SSE byte-stream decoder.
//!
//! [`SseStreamDecoder`] accumulates raw bytes from an HTTP response into a line
//! buffer, drains complete `data:` lines, and delegates frame parsing to
//! [`super::sse_parser`].  Its explicit state makes it straightforward to unit-
//! test without standing up an actual HTTP server or wrapping everything in an
//! `async_stream` macro block.

use anyhow::Result;
use tracing::debug;

use gglib_core::LlmStreamEvent;

use super::sse_parser::{SseParseResult, parse_sse_frame};

/// Stateful decoder that turns a sequence of raw SSE byte chunks into a
/// sequence of [`LlmStreamEvent`] values.
///
/// # Usage
///
/// ```ignore
/// let mut decoder = SseStreamDecoder::new();
/// while let Some(chunk) = byte_stream.next().await { … }
///     let (events, stop) = decoder.feed_bytes(&chunk);
///     for event in events { … }
///     if stop { break; }
/// }
/// if let Some(fallback) = decoder.finish() { … }
/// ```
#[derive(Default)]
pub(crate) struct SseStreamDecoder {
    buf: String,
    /// Set to `true` once a [`LlmStreamEvent::Done`] has been yielded, so the
    /// `[DONE]` sentinel doesn't generate a duplicate.
    done_sent: bool,
}

impl SseStreamDecoder {
    pub(crate) fn new() -> Self {
        Self::default()
    }
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
    pub(crate) fn feed_bytes(&mut self, bytes: &[u8]) -> (Vec<Result<LlmStreamEvent>>, bool) {
        let text = match std::str::from_utf8(bytes) {
            Ok(t) => t,
            Err(e) => {
                return (
                    vec![Err(anyhow::anyhow!("invalid UTF-8 in LLM SSE stream: {e}"))],
                    true,
                );
            }
        };
        self.buf.push_str(text);
        let mut events = Vec::new();

        loop {
            let Some(newline_pos) = self.buf.find('\n') else {
                break;
            };
            let line = self.buf[..newline_pos].trim_end_matches('\r').to_owned();
            self.buf.drain(..=newline_pos);

            // Skip blank lines and SSE comment lines.
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };

            match parse_sse_frame(data) {
                Ok(SseParseResult::Done) => {
                    if !self.done_sent {
                        debug!(
                            "LLM stream ended with [DONE] but no prior finish_reason \
                             — emitting fallback Done"
                        );
                        events.push(Ok(LlmStreamEvent::Done {
                            finish_reason: "stop".to_owned(),
                        }));
                    }
                    self.done_sent = true;
                    return (events, true);
                }
                Ok(SseParseResult::Events(parsed_events)) => {
                    for event in parsed_events {
                        if matches!(event, LlmStreamEvent::Done { .. }) {
                            self.done_sent = true;
                        }
                        events.push(Ok(event));
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
    /// Returns `None` if a `Done` was already yielded by [`feed_bytes`].
    pub(crate) fn finish(self) -> Option<LlmStreamEvent> {
        if !self.done_sent {
            debug!("LLM byte-stream ended without [DONE] sentinel — emitting fallback Done");
            Some(LlmStreamEvent::Done {
                finish_reason: "stop".to_owned(),
            })
        } else {
            None
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use gglib_core::LlmStreamEvent;

    use super::SseStreamDecoder;

    fn text_delta_frame(text: &str) -> String {
        let json = serde_json::json!({
            "choices": [{
                "delta": { "content": text },
                "finish_reason": null
            }]
        });
        format!("data: {json}\n")
    }

    fn done_frame() -> &'static str {
        "data: [DONE]\n"
    }

    fn finish_reason_frame() -> String {
        let json = serde_json::json!({
            "choices": [{
                "delta": {},
                "finish_reason": "stop"
            }]
        });
        format!("data: {json}\n")
    }

    // ---- helpers ------------------------------------------------------------

    fn collect_all(decoder: &mut SseStreamDecoder, input: &str) -> (Vec<LlmStreamEvent>, bool) {
        let (raw, stop) = decoder.feed_bytes(input.as_bytes());
        let events: Vec<_> = raw.into_iter().map(Result::unwrap).collect();
        (events, stop)
    }

    // ---- tests --------------------------------------------------------------

    #[test]
    fn text_delta_is_emitted() {
        let mut dec = SseStreamDecoder::new();
        let (events, stop) = collect_all(&mut dec, &text_delta_frame("hello"));
        assert!(!stop);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, LlmStreamEvent::TextDelta { content } if content == "hello"))
        );
    }

    #[test]
    fn done_sentinel_signals_stop_and_emits_fallback() {
        let mut dec = SseStreamDecoder::new();
        let (events, stop) = collect_all(&mut dec, done_frame());
        assert!(stop, "decoder should signal stop on [DONE]");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, LlmStreamEvent::Done { .. })),
            "fallback Done should be emitted when no prior finish_reason"
        );
        // After a [DONE] sentinel, finish() must not emit a second Done.
        assert!(
            dec.finish().is_none(),
            "finish() must return None after a [DONE] sentinel — done_sent must be set"
        );
    }

    #[test]
    fn finish_reason_then_done_no_duplicate_done() {
        let mut dec = SseStreamDecoder::new();
        let input = format!("{}{}", finish_reason_frame(), done_frame());
        let (events, stop) = collect_all(&mut dec, &input);
        assert!(stop);
        let done_count = events
            .iter()
            .filter(|e| matches!(e, LlmStreamEvent::Done { .. }))
            .count();
        assert_eq!(done_count, 1, "exactly one Done should be emitted");
    }

    #[test]
    fn finish_emits_fallback_when_stream_ends_without_done() {
        let mut dec = SseStreamDecoder::new();
        // Feed a text delta but no Done frame.
        let _ = collect_all(&mut dec, &text_delta_frame("partial"));
        let fallback = dec.finish();
        assert!(
            fallback.is_some(),
            "finish() should return a fallback Done when stream ends without one"
        );
    }

    #[test]
    fn finish_returns_none_when_done_already_sent() {
        let mut dec = SseStreamDecoder::new();
        let _ = collect_all(&mut dec, &finish_reason_frame());
        // done_sent is now true
        assert!(
            dec.finish().is_none(),
            "finish() must not emit a second Done"
        );
    }

    #[test]
    fn partial_line_buffered_until_newline_arrives() {
        let mut dec = SseStreamDecoder::new();
        let full_frame = text_delta_frame("world");

        // Split the frame across two feed calls.
        let mid = full_frame.len() / 2;
        let (first_events, stop1) = collect_all(&mut dec, &full_frame[..mid]);
        assert!(!stop1);
        assert!(first_events.is_empty(), "no complete line yet");

        let (second_events, stop2) = collect_all(&mut dec, &full_frame[mid..]);
        assert!(!stop2);
        assert!(
            second_events
                .iter()
                .any(|e| matches!(e, LlmStreamEvent::TextDelta { .. })),
            "TextDelta should be emitted once the newline arrives"
        );
    }
}
