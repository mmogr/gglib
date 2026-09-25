//! The upstream's streamed reply, read into typed events under an idle bound,
//! and the `upstream_timeout` bodies a streaming client is sent when the
//! upstream goes quiet: before its reply begins, or partway through it.
//!
//! [`upstream_events`] is the first stage of the response pipeline in
//! [`crate::forward`]: llama-server's SSE bytes in, [`LlmStreamEvent`]s out,
//! ready for the normalizer. A read that waits longer than
//! [`StreamBounds::idle`] ends the events with [`UpstreamStalled`].
//! [`first_byte_timeout_frame`] is what the client gets instead of a reply
//! when the upstream sent no response headers before the first-byte deadline,
//! no other request was active, and the attempt was the last one or a recycle
//! was pending.
//! [`UpstreamStalled::notice`] and [`UpstreamStalled::error_frame`] are what
//! it gets when a reply stops partway.

use std::time::Duration;

use bytes::Bytes;
use futures_util::{Stream, StreamExt as _};
use tracing::warn;

use gglib_core::LlmStreamEvent;
use gglib_core::sse::SseStreamDecoder;

use crate::forward::{FIRST_BYTE_DEADLINE_SECS, visible_content_frame};

/// How long one read of a streamed reply may wait for the upstream's next
/// bytes before the turn is ended as stalled.
///
/// It times reads, not the turn. The timer starts when the drain asks for the
/// next chunk and stops when one arrives. While the drain waits on the client
/// (`tx.send`) or on a repair re-issue, the reader is parked at its `yield` and
/// no read is being timed, so a slow client is never mistaken for a silent
/// upstream. The converse holds too: this bound does not notice a client that
/// stops reading. What notices a client that leaves is described on
/// [`drain_events`](crate::forward::drain_events).
///
/// Prefill sets the floor. The proxy asks llama-server for `return_progress`
/// (see `inject_streaming_body_overrides` in [`crate::forward`]), so it sends
/// a `prompt_progress` chunk after each batch of the prompt. gglib passes no
/// `-b` or `-ub` and `extra_args` has no path from the user, so a batch is
/// llama-server's default 2048 tokens, and a host that prefills slower than
/// about 6.8 tokens a second (2048 / 300) can go longer than this between two
/// chunks. Such a stall comes before the first token, so it strikes once
/// rather than recycling the model; see
/// [`StreamVerdict::Stalled`](crate::upstream_health::StreamVerdict::Stalled).
pub(crate) const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// How long a streamed chat completion waits on a silent upstream.
///
/// [`serve`](crate::serve) runs with [`StreamBounds::default`]. A test build
/// can start it inside `TEST_STREAM_BOUNDS.scope(bounds, ..)` to run it with
/// shorter ones; nothing else changes them.
#[cfg_attr(not(any(test, feature = "test-support")), allow(unreachable_pub))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamBounds {
    /// How long to wait for response headers, per cycle; see
    /// [`FIRST_BYTE_DEADLINE_SECS`].
    pub first_byte: Duration,
    /// How long one read of the reply may wait; see [`STREAM_IDLE_TIMEOUT`].
    pub idle: Duration,
}

impl Default for StreamBounds {
    fn default() -> Self {
        Self {
            first_byte: Duration::from_secs(FIRST_BYTE_DEADLINE_SECS),
            idle: STREAM_IDLE_TIMEOUT,
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
tokio::task_local! {
    /// The bounds a [`serve`](crate::serve) started inside
    /// `TEST_STREAM_BOUNDS.scope(bounds, ..)` runs under, in place of the
    /// defaults. Test builds only: a test cannot wait five minutes for a stall.
    pub static TEST_STREAM_BOUNDS: StreamBounds;
}

impl StreamBounds {
    /// The bounds [`serve`](crate::serve) runs under: the defaults, unless a
    /// test build started it inside `TEST_STREAM_BOUNDS.scope(..)`.
    pub(crate) fn for_serve() -> Self {
        #[cfg(any(test, feature = "test-support"))]
        if let Ok(bounds) = TEST_STREAM_BOUNDS.try_with(|bounds| *bounds) {
            return bounds;
        }
        Self::default()
    }
}

/// The upstream sent nothing for [`StreamBounds::idle`] while the proxy waited
/// for the next chunk of its reply.
///
/// [`upstream_events`] ends with this as its `Err`, so the drain tells a stall
/// from a transport error by downcasting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UpstreamStalled {
    /// How long the upstream had been silent: the idle bound.
    pub(crate) after: Duration,
    /// Whether a generated token (content, reasoning or a tool call) had
    /// arrived before the silence, which means prefill was over.
    pub(crate) after_first_token: bool,
}

impl std::fmt::Display for UpstreamStalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let secs = self.after.as_secs();
        write!(f, "upstream sent nothing for {secs}s mid-response")
    }
}

impl std::error::Error for UpstreamStalled {}

impl UpstreamStalled {
    /// The notice a person reads in the chat pane, sent as the turn's own text.
    ///
    /// It says the model is being recycled only when it is: a stall after the
    /// first token asks for a recycle at once, and one before it only strikes.
    pub(crate) fn notice(&self) -> String {
        let secs = self.after.as_secs();
        let recycling = if self.after_first_token {
            "; this model is being recycled"
        } else {
            ""
        };
        format!(
            "\n\n⚠️ [proxy] upstream model server went silent for {secs}s mid-response — it may be wedged{recycling}."
        )
    }

    /// The `upstream_timeout` error frame that ends a stalled turn. It carries
    /// no `[DONE]`: the drain sends that once, after everything else.
    pub(crate) fn error_frame(&self) -> String {
        timeout_error_frame(&self.to_string())
    }
}

/// Decode an upstream SSE byte stream into typed events, waiting at most
/// `idle` for each read.
///
/// Reading stops at the end of the body or where the decoder says the stream
/// ended (the `[DONE]` sentinel, an inline upstream error, or bytes it cannot
/// decode), and nothing after that is read. Unless a `Done` or an inline
/// upstream error has already ended the turn, the decoder then adds a
/// fallback `Done`.
///
/// A transport error or a stall is different: the stream ends with one `Err`,
/// after every event already decoded, and no `Done` follows it; the `Err` is
/// how the turn ends. A stall's `Err` is an [`UpstreamStalled`]. The timer
/// runs only while a read is waiting (see [`STREAM_IDLE_TIMEOUT`]). The drain
/// ([`drain_events`](crate::forward::drain_events)) reads these events through
/// the normalizer
/// ([`NormalizingStream`](gglib_core::normalize::NormalizingStream)), which
/// stops at the first `Err`, so it would see nothing after it either way.
pub(crate) fn upstream_events<S, E>(
    bytes: S,
    idle: Duration,
) -> impl Stream<Item = anyhow::Result<LlmStreamEvent>>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: std::fmt::Display,
{
    async_stream::stream! {
        let mut decoder = SseStreamDecoder::default();
        let mut byte_stream = std::pin::pin!(bytes);
        let mut after_first_token = false;

        loop {
            let Ok(next) = tokio::time::timeout(idle, byte_stream.next()).await else {
                let stall = UpstreamStalled { after: idle, after_first_token };
                warn!("{stall}");
                yield Err(anyhow::Error::new(stall));
                return;
            };
            let Some(chunk_result) = next else {
                break;
            };
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    warn!("upstream SSE byte-stream error: {e}");
                    yield Err(anyhow::anyhow!("upstream SSE byte-stream error: {e}"));
                    return;
                }
            };
            let (events, stop) = decoder.feed_bytes(&chunk);
            for event in events {
                after_first_token |= event.as_ref().is_ok_and(is_generated_token);
                yield event;
            }
            if stop {
                break;
            }
        }

        if let Some(fallback) = decoder.finish() {
            yield Ok(fallback);
        }
    }
}

/// Whether `event` is a generated token: content, reasoning or a tool call.
/// Once one has arrived, prefill is over. It is the test behind
/// [`UpstreamStalled::after_first_token`], and the drain applies it to the
/// same events, before the normalizer can hold any back.
pub(crate) fn is_generated_token(event: &LlmStreamEvent) -> bool {
    matches!(
        event,
        LlmStreamEvent::TextDelta { .. }
            | LlmStreamEvent::ReasoningDelta { .. }
            | LlmStreamEvent::ToolCallDelta { .. }
    )
}

/// One `upstream_timeout` error frame carrying `message`: the one place the
/// proxy writes that code. Clients match on it (ggchat reads it as "wait and
/// retry") and show the message.
fn timeout_error_frame(message: &str) -> String {
    let payload = serde_json::json!({
        "error": {
            "message": message,
            "type": "server_error",
            "code": "upstream_timeout",
        }
    });
    format!("data: {payload}\n\n")
}

/// The body a streaming client is sent when the first-byte deadline (`after`)
/// expires with no other request active, on its last attempt or on any
/// attempt while a recycle is pending: a visible notice, the
/// `upstream_timeout` error frame, then `[DONE]`.
///
/// The notice rides alongside because some clients do not render an inline
/// error frame at all; see [`visible_content_frame`].
pub(crate) fn first_byte_timeout_frame(model: &str, after: Duration) -> String {
    let secs = after.as_secs();
    let visible = visible_content_frame(
        model,
        &format!(
            "⚠️ [proxy] upstream model server did not begin responding within {secs}s — it may be overloaded or wedged. Retry; if it persists the model will be recycled."
        ),
    );
    let error = timeout_error_frame(&format!("upstream did not respond within {secs}s"));
    format!("{visible}{error}data: [DONE]\n\n")
}

#[cfg(test)]
#[path = "upstream_read_tests.rs"]
mod tests;
