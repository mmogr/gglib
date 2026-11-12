//! The upstream's streamed reply, read into frames under an idle bound, and
//! the `upstream_timeout` bodies a streaming client is sent when the upstream
//! goes quiet: before its reply begins, or partway through it.
//!
//! [`upstream_frames`] is the first stage of the response pipeline in
//! [`crate::forward`]: llama-server's SSE bytes in, [`SseItem`]s out, each
//! frame with its JSON and its events, for the drain to forward. A reply that
//! goes longer than [`StreamBounds::idle`] without its next frame (before the
//! first token, without any bytes) ends the items with [`UpstreamStalled`].
//! [`first_byte_timeout_frame`] is what the client gets instead of a reply
//! when the upstream sent no response headers before the first-byte deadline,
//! no other request was active, and the attempt was the last one or a recycle
//! was pending.
//! [`UpstreamStalled::notice`] and [`UpstreamStalled::error_frame`] are what
//! it gets when a reply stops partway, and [`prefill_comment`] what a client
//! that did not ask for progress gets in place of each `prompt_progress`
//! frame.

use std::time::Duration;

use bytes::Bytes;
use futures_util::{Stream, StreamExt as _};
use tracing::warn;

use gglib_core::LlmStreamEvent;
use gglib_core::sse::{SseEncoder, SseItem, SseStreamDecoder};

use crate::forward::{FIRST_BYTE_DEADLINE_SECS, failed_turn_body};

/// How long a streamed reply may wait for the upstream's next frame before
/// the turn is ended as stalled.
///
/// It times reads, not the turn. The timer starts when the drain asks for the
/// next item and stops when a chunk holds a `data:` frame. Once the first
/// token has come, a chunk with none, such as the `:` keep-alive comment
/// llama-server sends while it works, leaves it running: the comment is
/// passed on to the client, but a reply that only pings is still judged
/// stalled. While the drain waits on the client (`tx.send`) or on a repair
/// re-issue, the reader is parked at its `yield` and no read is being timed,
/// so a slow client is never mistaken for a silent upstream. The converse
/// holds too: this bound does not notice a client that stops reading, which
/// its twin on the client's side,
/// [`CLIENT_SEND_TIMEOUT`](crate::client_send::CLIENT_SEND_TIMEOUT), does.
///
/// Before the first token, any bytes stop it. The proxy asks llama-server for
/// `return_progress` (see `inject_streaming_body_overrides` in
/// [`crate::forward`]), so it sends a `prompt_progress` chunk after each
/// 2048-token batch of the prompt and only keep-alive comments between them;
/// counting those keeps a slow prefill from being cut. A stall before the
/// first token strikes once rather than recycling the model; see
/// [`StreamVerdict::Stalled`](crate::upstream_health::StreamVerdict::Stalled).
pub const STREAM_IDLE_TIMEOUT: Duration = Duration::from_mins(5);

/// How long the proxy waits on a silent upstream, on a streaming client that
/// stopped reading, and on a request that does not stream.
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
    /// How long the reply may wait for its next frame; see
    /// [`STREAM_IDLE_TIMEOUT`].
    pub idle: Duration,
    /// How long one send to a streaming client may wait for it to make room;
    /// see [`CLIENT_SEND_TIMEOUT`](crate::client_send::CLIENT_SEND_TIMEOUT).
    pub send: Duration,
    /// How long a chat completion that does not stream, or an embeddings
    /// request, may take in all; see
    /// [`UNARY_TOTAL_TIMEOUT`](crate::unary_body::UNARY_TOTAL_TIMEOUT).
    pub unary: Duration,
}

impl Default for StreamBounds {
    fn default() -> Self {
        Self {
            first_byte: Duration::from_secs(FIRST_BYTE_DEADLINE_SECS),
            idle: STREAM_IDLE_TIMEOUT,
            send: crate::client_send::CLIENT_SEND_TIMEOUT,
            unary: crate::unary_body::UNARY_TOTAL_TIMEOUT,
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

/// The upstream sent no frame for [`StreamBounds::idle`] while the proxy
/// waited for the next one: keep-alive comments aside once tokens flow, no
/// bytes at all before the first.
///
/// [`upstream_frames`] ends with this as its `Err`, so the drain tells a stall
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

/// Read an upstream SSE byte stream into [`SseItem`]s, waiting at most `idle`
/// for each next frame (before the first token, for any bytes).
///
/// Reading stops at the end of the body or where the decoder says the stream
/// ended (`[DONE]`, a frame with an inline upstream error, or bytes it cannot
/// decode), and nothing after that is read. Nothing is added at the end: a
/// body that stops without a finish frame ends with its last item.
///
/// A transport error or a stall ends the items with one `Err`, after every
/// item already read; a stall's is an [`UpstreamStalled`]. The timer runs only
/// while a read is waiting (see [`STREAM_IDLE_TIMEOUT`]), and only a `data:`
/// frame restarts it once the first token has come: a comment is passed on,
/// but it is not the reply moving. The drain
/// ([`drain_events`](crate::forward::drain_events)) reads these items as they
/// come, so the `Err` is the last thing it sees.
pub(crate) fn upstream_frames<S, E>(
    bytes: S,
    idle: Duration,
) -> impl Stream<Item = anyhow::Result<SseItem>>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: std::fmt::Display,
{
    async_stream::stream! {
        let mut decoder = SseStreamDecoder::default();
        let mut byte_stream = std::pin::pin!(bytes);
        let mut after_first_token = false;
        // When the wait for the next frame ends. Restored once the items of
        // a chunk have been taken. After the first token it is kept across a
        // chunk with no frame in it, so keep-alive comments cannot hold a
        // silent reply open; before it they are all prefill sends.
        let mut deadline = tokio::time::Instant::now() + idle;

        loop {
            let Ok(next) = tokio::time::timeout_at(deadline, byte_stream.next()).await else {
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
            let (items, stop) = decoder.feed_frames(&chunk);
            let moved = items.iter().any(|item| matches!(item, Ok(SseItem::Data { .. })));
            for item in items {
                if let Ok(SseItem::Data { events, .. }) = &item {
                    after_first_token |= events.iter().any(is_generated_token);
                }
                yield item;
            }
            if stop {
                break;
            }
            if moved || !after_first_token {
                deadline = tokio::time::Instant::now() + idle;
            }
        }
    }
}

/// Whether `event` is a generated token: content, reasoning or a tool call.
/// Once one has arrived, prefill is over. It is the test behind
/// [`UpstreamStalled::after_first_token`], and the drain applies it to the
/// same events as they arrive: to tell a client
/// that left before the first token, and to stamp the clock behind a turn's
/// `generation_ms`. It reads the kind of event, not what the event carries.
pub(crate) fn is_generated_token(event: &LlmStreamEvent) -> bool {
    matches!(
        event,
        LlmStreamEvent::TextDelta { .. }
            | LlmStreamEvent::ReasoningDelta { .. }
            | LlmStreamEvent::ToolCallDelta { .. }
    )
}

/// The SSE comment, `: prefill <processed>/<total>`, that a client which did
/// not ask for progress is sent in place of a `prompt_progress` frame.
///
/// A long prefill would otherwise send that client nothing, and a client whose
/// inactivity timer resets only on received bytes gives up on a prefill longer
/// than that timer (#1213).
///
/// Invisible to clients: `parse_sse_frames` and every conforming SSE reader
/// skip lines that are not `data:`, so a client that did not order the frame
/// never reads one.
pub(crate) fn prefill_comment(processed: u32, total: u32) -> Bytes {
    Bytes::from(format!(": prefill {processed}/{total}\n\n"))
}

/// One `upstream_timeout` error frame carrying `message`: the one place the
/// proxy writes that code. Clients match on it (ggchat reads it as "wait and
/// retry") and show the message.
fn timeout_error_frame(message: &str) -> String {
    SseEncoder::upstream_error_frame(message, "server_error", "upstream_timeout")
}

/// The body a streaming client is sent when the first-byte deadline (`after`)
/// expires with no other request active, on its last attempt or on any
/// attempt while a recycle is pending: a visible notice, the
/// `upstream_timeout` error frame, then `[DONE]`.
///
/// The notice rides alongside because some clients do not render an inline
/// error frame at all; see [`failed_turn_body`].
pub(crate) fn first_byte_timeout_frame(model: &str, after: Duration) -> String {
    let secs = after.as_secs();
    let notice = format!(
        "⚠️ [proxy] upstream model server did not begin responding within {secs}s — it may be overloaded or wedged. Retry; if it persists the model will be recycled."
    );
    let error = timeout_error_frame(&format!("upstream did not respond within {secs}s"));
    failed_turn_body(model, &notice, &error)
}

#[cfg(test)]
#[path = "upstream_read_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "upstream_frames_tests.rs"]
mod frames_tests;
