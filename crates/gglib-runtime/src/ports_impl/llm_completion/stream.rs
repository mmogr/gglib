//! Response SSE bytes → normalized `LlmStreamEvent`s, with an optional
//! token-usage tap.
//!
//! Kept separate from the adapter's request-shaping so the streaming concern —
//! SSE decode, parser normalization, and the telemetry tap — reads as one
//! unit and the adapter file stays focused on building the outgoing request.

use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use futures_core::Stream;
use futures_util::StreamExt as _;

use gglib_core::{
    domain::agent::LlmStreamEvent,
    domain::dialect::DialectSpec,
    normalize::{NormalizingStream, get_parser},
    ports::UsageSink,
    sse::SseStreamDecoder,
};

/// Boxed stream of decoded, normalized completion events.
pub(super) type EventStream = Pin<Box<dyn Stream<Item = Result<LlmStreamEvent>> + Send>>;

/// Turn an SSE byte response into the typed, normalized event stream the agent
/// loop consumes, optionally tapping the response's token usage into `sink`.
///
/// `dialect` selects the response parser — `None` selects the
/// identity-passthrough parser, so models that already emit strict OpenAI tool
/// calls are unaffected.
pub(super) fn normalized_event_stream(
    response: reqwest::Response,
    dialect: Option<&DialectSpec>,
    sink: Option<Arc<dyn UsageSink>>,
) -> EventStream {
    let byte_stream = response.bytes_stream();

    // Build the typed event stream from the raw SSE byte stream.
    let raw = async_stream::stream! {
        let mut decoder = SseStreamDecoder::default();
        let mut byte_stream = std::pin::pin!(byte_stream);

        'outer: while let Some(chunk_result) = byte_stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    // Attach the reqwest error rather than formatting it away.
                    // `Response::bytes_stream` funnels every body failure
                    // through `error::decode`, so an idle-timeout and a
                    // mid-stream connection reset share the Display string
                    // "error decoding response body" and are told apart only
                    // by the source beneath it. Formatting with `{e}` here
                    // discarded exactly that, and cost the 2026-08-28 eval a
                    // diagnosis: five runs died at a timeout nothing named.
                    // Readers must use `{:#}` to see the chain.
                    yield Err(anyhow::Error::new(e).context("SSE byte-stream error"));
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
    };

    let parser = get_parser(dialect);
    let normalized: EventStream = Box::pin(NormalizingStream::new(Box::pin(raw), parser));

    match sink {
        None => normalized,
        Some(sink) => tap_usage(normalized, sink),
    }
}

/// Telemetry-only tap on the fully-normalized stream: the single point that
/// covers every agent-path consumer without any of them knowing about it.
///
/// Records the last `Usage` frame once the stream drains — mirroring the
/// proxy's "last usage wins, record once" semantics — so a stream that carries
/// no usage records nothing, and `cached_tokens`' absent-vs-zero distinction
/// survives to the sink.
///
/// The tap sits after normalization but is independent of it: a raw-passthrough
/// request selects the identity parser, which forwards `Usage` verbatim, so the
/// control arm of an A/B evaluation is measured on the same footing as the
/// shaped one.
fn tap_usage(stream: EventStream, sink: Arc<dyn UsageSink>) -> EventStream {
    Box::pin(async_stream::stream! {
        let mut stream = std::pin::pin!(stream);
        let mut last_usage: Option<(u32, u32, Option<u32>)> = None;
        while let Some(item) = stream.next().await {
            if let Ok(LlmStreamEvent::Usage {
                prompt_tokens, completion_tokens, cached_tokens, ..
            }) = &item
            {
                last_usage = Some((*prompt_tokens, *completion_tokens, *cached_tokens));
            }
            yield item;
        }
        if let Some((prompt_tokens, completion_tokens, cached_tokens)) = last_usage {
            sink.record(prompt_tokens, completion_tokens, cached_tokens);
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Records `(prompt_tokens, completion_tokens, cached_tokens)` per call.
    #[derive(Default)]
    struct FakeSink(Mutex<Vec<(u32, u32, Option<u32>)>>);

    impl UsageSink for FakeSink {
        fn record(&self, prompt_tokens: u32, completion_tokens: u32, cached_tokens: Option<u32>) {
            self.0
                .lock()
                .unwrap()
                .push((prompt_tokens, completion_tokens, cached_tokens));
        }
    }

    fn events(items: Vec<LlmStreamEvent>) -> EventStream {
        Box::pin(futures_util::stream::iter(items.into_iter().map(Ok)))
    }

    fn usage(prompt: u32, cached: Option<u32>) -> LlmStreamEvent {
        usage_with_completion(prompt, 0, cached)
    }

    fn usage_with_completion(prompt: u32, completion: u32, cached: Option<u32>) -> LlmStreamEvent {
        LlmStreamEvent::Usage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
            cached_tokens: cached,
        }
    }

    async fn drain(mut stream: EventStream) -> Vec<LlmStreamEvent> {
        let mut out = Vec::new();
        while let Some(item) = stream.next().await {
            out.push(item.expect("no stream error in these fixtures"));
        }
        out
    }

    #[tokio::test]
    async fn passes_every_event_through_and_records_reported_usage() {
        let sink = Arc::new(FakeSink::default());
        let tapped = tap_usage(
            events(vec![
                LlmStreamEvent::TextDelta {
                    content: "hi".to_owned(),
                },
                usage(1_000, Some(900)),
                LlmStreamEvent::Done {
                    finish_reason: Some("stop".to_owned()),
                },
            ]),
            sink.clone(),
        );

        let passed = drain(tapped).await;
        assert_eq!(passed.len(), 3, "tap must forward every event unchanged");
        assert_eq!(*sink.0.lock().unwrap(), vec![(1_000, 0, Some(900))]);
    }

    #[tokio::test]
    async fn preserves_unreported_cached_tokens_as_none() {
        let sink = Arc::new(FakeSink::default());
        drain(tap_usage(events(vec![usage(500, None)]), sink.clone())).await;
        assert_eq!(*sink.0.lock().unwrap(), vec![(500, 0, None)]);
    }

    #[tokio::test]
    async fn zero_reuse_is_recorded_not_dropped() {
        let sink = Arc::new(FakeSink::default());
        drain(tap_usage(events(vec![usage(500, Some(0))]), sink.clone())).await;
        assert_eq!(*sink.0.lock().unwrap(), vec![(500, 0, Some(0))]);
    }

    #[tokio::test]
    async fn a_stream_without_usage_records_nothing() {
        let sink = Arc::new(FakeSink::default());
        drain(tap_usage(
            events(vec![LlmStreamEvent::Done {
                finish_reason: Some("stop".to_owned()),
            }]),
            sink.clone(),
        ))
        .await;
        assert!(sink.0.lock().unwrap().is_empty());
    }

    /// The generation-side count is what makes a guard-aborted benchmark task
    /// measurable, so it must reach the sink rather than being dropped with the
    /// rest of the frame.
    #[tokio::test]
    async fn completion_tokens_reach_the_sink() {
        let sink = Arc::new(FakeSink::default());
        drain(tap_usage(
            events(vec![usage_with_completion(120, 32_550, Some(64))]),
            sink.clone(),
        ))
        .await;
        assert_eq!(*sink.0.lock().unwrap(), vec![(120, 32_550, Some(64))]);
    }

    #[tokio::test]
    async fn only_the_last_usage_frame_is_recorded() {
        let sink = Arc::new(FakeSink::default());
        drain(tap_usage(
            events(vec![usage(1_000, Some(100)), usage(2_000, Some(1_500))]),
            sink.clone(),
        ))
        .await;
        assert_eq!(*sink.0.lock().unwrap(), vec![(2_000, 0, Some(1_500))]);
    }
}
