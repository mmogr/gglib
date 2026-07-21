//! Response SSE bytes → normalized `LlmStreamEvent`s, with an optional
//! prompt-cache usage tap.
//!
//! Kept separate from the adapter's request-shaping so the streaming concern —
//! SSE decode, parser normalization, and the telemetry tap — reads as one
//! unit and the adapter file stays focused on building the outgoing request.

use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use futures_core::Stream;
use futures_util::StreamExt as _;

use gglib_core::{
    domain::agent::LlmStreamEvent,
    normalize::{NormalizingStream, get_parser},
    ports::CacheMetricsSink,
    sse::SseStreamDecoder,
};

/// Boxed stream of decoded, normalized completion events.
pub(super) type EventStream = Pin<Box<dyn Stream<Item = Result<LlmStreamEvent>> + Send>>;

/// Turn an SSE byte response into the typed, normalized event stream the agent
/// loop consumes, optionally tapping prompt-cache reuse into `sink`.
///
/// `model_tags` selects the response parser — empty selects the
/// identity-passthrough parser, so models that already emit strict OpenAI tool
/// calls are unaffected.
pub(super) fn normalized_event_stream(
    response: reqwest::Response,
    model_tags: &[String],
    sink: Option<Arc<dyn CacheMetricsSink>>,
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
                    yield Err(anyhow!("SSE byte-stream error: {e}"));
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

    let parser = get_parser(model_tags);
    let normalized: EventStream = Box::pin(NormalizingStream::new(Box::pin(raw), parser));

    match sink {
        None => normalized,
        Some(sink) => tap_cache_usage(normalized, sink),
    }
}

/// Telemetry-only tap on the fully-normalized stream: the single point that
/// covers every agent-path consumer (both `stream_collector` and
/// `structured_output`) without either knowing about it.
///
/// Records the last `Usage` frame once the stream drains — mirroring the
/// proxy's "last usage wins, record once" semantics — so a stream that carries
/// no usage records nothing, and the `Option<u32>` absent-vs-zero distinction
/// survives to the sink.
fn tap_cache_usage(stream: EventStream, sink: Arc<dyn CacheMetricsSink>) -> EventStream {
    Box::pin(async_stream::stream! {
        let mut stream = std::pin::pin!(stream);
        let mut last_usage: Option<(u32, Option<u32>)> = None;
        while let Some(item) = stream.next().await {
            if let Ok(LlmStreamEvent::Usage { prompt_tokens, cached_tokens, .. }) = &item {
                last_usage = Some((*prompt_tokens, *cached_tokens));
            }
            yield item;
        }
        if let Some((prompt_tokens, cached_tokens)) = last_usage {
            sink.record(prompt_tokens, cached_tokens);
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct FakeSink(Mutex<Vec<(u32, Option<u32>)>>);

    impl CacheMetricsSink for FakeSink {
        fn record(&self, prompt_tokens: u32, cached_tokens: Option<u32>) {
            self.0.lock().unwrap().push((prompt_tokens, cached_tokens));
        }
    }

    fn events(items: Vec<LlmStreamEvent>) -> EventStream {
        Box::pin(futures_util::stream::iter(items.into_iter().map(Ok)))
    }

    fn usage(prompt: u32, cached: Option<u32>) -> LlmStreamEvent {
        LlmStreamEvent::Usage {
            prompt_tokens: prompt,
            completion_tokens: 0,
            total_tokens: prompt,
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
        let tapped = tap_cache_usage(
            events(vec![
                LlmStreamEvent::TextDelta {
                    content: "hi".to_owned(),
                },
                usage(1_000, Some(900)),
                LlmStreamEvent::Done {
                    finish_reason: "stop".to_owned(),
                },
            ]),
            sink.clone(),
        );

        let passed = drain(tapped).await;
        assert_eq!(passed.len(), 3, "tap must forward every event unchanged");
        assert_eq!(*sink.0.lock().unwrap(), vec![(1_000, Some(900))]);
    }

    #[tokio::test]
    async fn preserves_unreported_cached_tokens_as_none() {
        let sink = Arc::new(FakeSink::default());
        drain(tap_cache_usage(
            events(vec![usage(500, None)]),
            sink.clone(),
        ))
        .await;
        assert_eq!(*sink.0.lock().unwrap(), vec![(500, None)]);
    }

    #[tokio::test]
    async fn zero_reuse_is_recorded_not_dropped() {
        let sink = Arc::new(FakeSink::default());
        drain(tap_cache_usage(
            events(vec![usage(500, Some(0))]),
            sink.clone(),
        ))
        .await;
        assert_eq!(*sink.0.lock().unwrap(), vec![(500, Some(0))]);
    }

    #[tokio::test]
    async fn a_stream_without_usage_records_nothing() {
        let sink = Arc::new(FakeSink::default());
        drain(tap_cache_usage(
            events(vec![LlmStreamEvent::Done {
                finish_reason: "stop".to_owned(),
            }]),
            sink.clone(),
        ))
        .await;
        assert!(sink.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn only_the_last_usage_frame_is_recorded() {
        let sink = Arc::new(FakeSink::default());
        drain(tap_cache_usage(
            events(vec![usage(1_000, Some(100)), usage(2_000, Some(1_500))]),
            sink.clone(),
        ))
        .await;
        assert_eq!(*sink.0.lock().unwrap(), vec![(2_000, Some(1_500))]);
    }
}
