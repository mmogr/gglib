//! SSE stream sanitizer for strict OpenAI-compatible clients.
//!
//! This module strips `reasoning_content` fields from streamed JSON chunks
//! while preserving SSE framing and non-JSON events (for example `[DONE]`).

use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::{Bytes, BytesMut};
use futures_util::Stream;

/// Stateful SSE frame sanitizer that removes `reasoning_content` keys.
#[derive(Default)]
pub struct SseReasoningStripper {
    buffer: BytesMut,
}

impl SseReasoningStripper {
    /// Feed an upstream chunk and emit zero or more sanitized SSE frames.
    pub fn push_chunk(&mut self, chunk: &[u8]) -> std::io::Result<Vec<Bytes>> {
        self.buffer.extend_from_slice(chunk);

        let mut out = Vec::new();
        while let Some((idx, delim_len)) = find_sse_delimiter(&self.buffer) {
            let frame_with_delim = self.buffer.split_to(idx + delim_len);
            let frame_end = frame_with_delim.len() - delim_len;
            let frame = &frame_with_delim[..frame_end];
            let delimiter = &frame_with_delim[frame_end..];
            out.push(sanitize_sse_frame(frame, delimiter)?);
        }

        Ok(out)
    }

    /// Flush trailing bytes when the upstream stream ends.
    pub fn finish(&mut self) -> std::io::Result<Vec<Bytes>> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }

        let remaining = self.buffer.split().freeze();
        Ok(vec![sanitize_sse_frame(&remaining, b"")?])
    }
}

/// Stream wrapper that applies [`SseReasoningStripper`] to SSE bytes.
pub struct ReasoningContentStripStream<S> {
    inner: S,
    stripper: SseReasoningStripper,
    pending: VecDeque<std::io::Result<Bytes>>,
    finished: bool,
}

impl<S> ReasoningContentStripStream<S> {
    #[must_use]
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            stripper: SseReasoningStripper::default(),
            pending: VecDeque::new(),
            finished: false,
        }
    }
}

impl<S, E> Stream for ReasoningContentStripStream<S>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    type Item = std::io::Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(item) = self.pending.pop_front() {
                return Poll::Ready(Some(item));
            }

            if self.finished {
                return Poll::Ready(None);
            }

            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(chunk))) => match self.stripper.push_chunk(&chunk) {
                    Ok(chunks) => {
                        self.pending.extend(chunks.into_iter().map(Ok));
                    }
                    Err(e) => {
                        self.finished = true;
                        return Poll::Ready(Some(Err(e)));
                    }
                },
                Poll::Ready(Some(Err(e))) => {
                    self.finished = true;
                    return Poll::Ready(Some(Err(std::io::Error::other(e.to_string()))));
                }
                Poll::Ready(None) => {
                    self.finished = true;
                    match self.stripper.finish() {
                        Ok(chunks) => {
                            if chunks.is_empty() {
                                return Poll::Ready(None);
                            }
                            self.pending.extend(chunks.into_iter().map(Ok));
                        }
                        Err(e) => return Poll::Ready(Some(Err(e))),
                    }
                }
            }
        }
    }
}

fn sanitize_sse_frame(frame: &[u8], delimiter: &[u8]) -> std::io::Result<Bytes> {
    let frame_text = match std::str::from_utf8(frame) {
        Ok(text) => text,
        Err(_) => {
            let mut passthrough = Vec::with_capacity(frame.len() + delimiter.len());
            passthrough.extend_from_slice(frame);
            passthrough.extend_from_slice(delimiter);
            return Ok(Bytes::from(passthrough));
        }
    };

    let mut lines = Vec::new();
    for line in frame_text.split('\n') {
        let had_cr = line.ends_with('\r');
        let line_core = line.strip_suffix('\r').unwrap_or(line);
        let mut sanitized = sanitize_sse_line(line_core)?;
        if had_cr {
            sanitized.push('\r');
        }
        lines.push(sanitized);
    }

    let mut out = lines.join("\n").into_bytes();
    out.extend_from_slice(delimiter);
    Ok(Bytes::from(out))
}

fn sanitize_sse_line(line: &str) -> std::io::Result<String> {
    let Some(payload) = line.strip_prefix("data:") else {
        return Ok(line.to_owned());
    };

    let payload = payload.strip_prefix(' ').unwrap_or(payload);
    if payload.is_empty() || payload == "[DONE]" {
        return Ok(line.to_owned());
    }

    let mut value: serde_json::Value = match serde_json::from_str(payload) {
        Ok(value) => value,
        Err(_) => return Ok(line.to_owned()),
    };

    strip_reasoning_content(&mut value);
    let json = serde_json::to_string(&value).map_err(std::io::Error::other)?;
    Ok(format!("data: {json}"))
}

fn strip_reasoning_content(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("reasoning_content");
            for v in map.values_mut() {
                strip_reasoning_content(v);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                strip_reasoning_content(item);
            }
        }
        _ => {}
    }
}

fn find_sse_delimiter(buf: &[u8]) -> Option<(usize, usize)> {
    if buf.len() < 2 {
        return None;
    }

    let mut i = 0;
    while i + 1 < buf.len() {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some((i, 2));
        }
        if i + 3 < buf.len()
            && buf[i] == b'\r'
            && buf[i + 1] == b'\n'
            && buf[i + 2] == b'\r'
            && buf[i + 3] == b'\n'
        {
            return Some((i, 4));
        }
        i += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{StreamExt, stream};

    #[test]
    fn strips_reasoning_content_from_stream_frame() {
        let mut stripper = SseReasoningStripper::default();
        let input = b"data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think\",\"content\":\"ok\"},\"finish_reason\":null}]}\n\n";

        let out = stripper.push_chunk(input).expect("strip failed");
        assert_eq!(out.len(), 1);

        let text = String::from_utf8(out[0].to_vec()).expect("utf8");
        assert!(!text.contains("reasoning_content"));
        assert!(text.contains("\"content\":\"ok\""));
        assert!(text.ends_with("\n\n"));
    }

    #[test]
    fn handles_split_frames_across_chunks() {
        let mut stripper = SseReasoningStripper::default();
        let first = b"data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"t";
        let second = b"hink\",\"content\":\"ok\"},\"finish_reason\":null}]}\n\n";

        let out1 = stripper.push_chunk(first).expect("first chunk failed");
        assert!(out1.is_empty());

        let out2 = stripper.push_chunk(second).expect("second chunk failed");
        assert_eq!(out2.len(), 1);
        let text = String::from_utf8(out2[0].to_vec()).expect("utf8");
        assert!(!text.contains("reasoning_content"));
        assert!(text.contains("\"content\":\"ok\""));
    }

    #[test]
    fn preserves_done_sentinel() {
        let mut stripper = SseReasoningStripper::default();
        let input = b"data: [DONE]\n\n";

        let out = stripper.push_chunk(input).expect("strip failed");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].as_ref(), input);
    }

    #[tokio::test]
    async fn stream_wrapper_sanitizes_and_preserves_order() {
        let upstream = stream::iter(vec![
            Ok::<Bytes, std::io::Error>(Bytes::from_static(
                b"data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"a\",\"content\":\"A\"},\"finish_reason\":null}]}\n\n",
            )),
            Ok::<Bytes, std::io::Error>(Bytes::from_static(b"data: [DONE]\n\n")),
        ]);

        let mut wrapped = ReasoningContentStripStream::new(upstream);

        let first = wrapped
            .next()
            .await
            .expect("first item")
            .expect("first ok");
        let first_text = String::from_utf8(first.to_vec()).expect("utf8");
        assert!(!first_text.contains("reasoning_content"));
        assert!(first_text.contains("\"content\":\"A\""));

        let second = wrapped
            .next()
            .await
            .expect("second item")
            .expect("second ok");
        assert_eq!(second.as_ref(), b"data: [DONE]\n\n");

        assert!(wrapped.next().await.is_none());
    }
}
