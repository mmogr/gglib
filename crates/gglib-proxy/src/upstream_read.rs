//! The upstream's streamed reply, read into typed events, and the body a
//! streaming client is sent when the first-byte deadline expires on its last
//! attempt with no other request active.
//!
//! [`upstream_events`] is the first stage of the response pipeline in
//! [`crate::forward`]: llama-server's SSE bytes in, [`LlmStreamEvent`]s out,
//! ready for the normalizer. [`first_byte_timeout_frame`] is what the client
//! gets instead of a reply in that one case: on the last attempt the upstream
//! sent no response headers before the first-byte deadline, and no other
//! request was active.

use bytes::Bytes;
use futures_util::{Stream, StreamExt as _};
use tracing::warn;

use gglib_core::LlmStreamEvent;
use gglib_core::sse::SseStreamDecoder;

use crate::forward::{FIRST_BYTE_DEADLINE_SECS, visible_content_frame};

/// Decode an upstream SSE byte stream into typed events.
///
/// Reading stops at the end of the body or where the decoder says the stream
/// ended (the `[DONE]` sentinel, an inline upstream error, or bytes it cannot
/// decode), and nothing after that is read. Unless a `Done` or an inline
/// upstream error has already ended the turn, the decoder then adds a
/// fallback `Done`.
///
/// A transport error is different: the stream ends with one `Err`, after
/// every event already decoded, and no `Done` follows it; the `Err` is how
/// the turn ends. The drain
/// ([`stream_response_to_channel`](crate::forward::stream_response_to_channel))
/// reads these events through the normalizer
/// ([`NormalizingStream`](gglib_core::normalize::NormalizingStream)), which
/// stops at the first `Err`, so it would see nothing after it either way.
pub(crate) fn upstream_events<S, E>(bytes: S) -> impl Stream<Item = anyhow::Result<LlmStreamEvent>>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: std::fmt::Display,
{
    async_stream::stream! {
        let mut decoder = SseStreamDecoder::default();
        let mut byte_stream = std::pin::pin!(bytes);

        'outer: while let Some(chunk_result) = byte_stream.next().await {
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
                yield event;
            }
            if stop {
                break 'outer;
            }
        }

        if let Some(fallback) = decoder.finish() {
            yield Ok(fallback);
        }
    }
}

/// The body a streaming client is sent when the first-byte deadline
/// ([`FIRST_BYTE_DEADLINE_SECS`]) expires on its last attempt with no other
/// request active: a visible notice, the `upstream_timeout` error frame, then
/// `[DONE]`.
///
/// This is the one place the proxy writes `upstream_timeout`. Clients match
/// on that code (ggchat reads it as "wait and retry") and show the message.
/// The notice rides alongside because some clients do not render an inline
/// error frame at all; see [`visible_content_frame`].
pub(crate) fn first_byte_timeout_frame(model: &str) -> String {
    let visible = visible_content_frame(
        model,
        &format!(
            "⚠️ [proxy] upstream model server did not begin responding within {FIRST_BYTE_DEADLINE_SECS}s — it may be overloaded or wedged. Retry; if it persists the model will be recycled."
        ),
    );
    let payload = serde_json::json!({
        "error": {
            "message": format!(
                "upstream did not respond within {FIRST_BYTE_DEADLINE_SECS}s"
            ),
            "type": "server_error",
            "code": "upstream_timeout",
        }
    });
    format!("{visible}data: {payload}\n\ndata: [DONE]\n\n")
}

#[cfg(test)]
#[path = "upstream_read_tests.rs"]
mod tests;
