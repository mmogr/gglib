#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

//! Generic, transport-agnostic-within-axum SSE broadcast utility.
//!
//! [`Broadcaster<T>`] wraps a [`tokio::sync::broadcast`] channel and exposes
//! it as an Axum [`Sse`] response for any `T: Clone + Serialize`. It exists so
//! that the exact same broadcast+encode+keep-alive plumbing can be shared by
//! multiple crates (currently `gglib-axum`'s [`AppEvent`](https://docs.rs/gglib-core)
//! stream and `gglib-proxy`'s dashboard event stream) without either crate
//! depending on the other, and without duplicating the logic a second time.
//!
//! This crate intentionally has **zero dependencies on any other `gglib-*`
//! crate** - it is a pure leaf, safe to be depended on from any layer of the
//! hexagonal architecture (see `crates/README.md`).

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::{self, Stream};
use serde::Serialize;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

/// Keep-alive cadence and comment text for an SSE subscription.
///
/// Sent periodically as SSE comment frames (`:<text>\n\n`) so that
/// intermediate proxies/load balancers don't time out an idle connection.
#[derive(Debug, Clone, Copy)]
pub struct SseOptions {
    pub keepalive_interval: Duration,
    pub keepalive_text: &'static str,
}

impl Default for SseOptions {
    fn default() -> Self {
        Self {
            keepalive_interval: Duration::from_secs(30),
            keepalive_text: "ping",
        }
    }
}

/// A generic broadcast-based SSE hub for any `Clone + Serialize` event type.
///
/// Wraps a [`tokio::sync::broadcast`] channel and converts it into Axum SSE
/// responses. Multiple clients can subscribe concurrently; a slow subscriber
/// that falls behind the channel capacity silently misses events (broadcast
/// `Lagged` errors are skipped, never surfaced as a stream error to the
/// client).
pub struct Broadcaster<T> {
    sender: broadcast::Sender<T>,
}

impl<T> Broadcaster<T>
where
    T: Clone + Serialize + Send + Sync + 'static,
{
    /// Create a new broadcaster with the given channel capacity.
    ///
    /// `capacity` is the number of events that can be buffered for the
    /// slowest subscriber before it starts missing events.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish an event to all current subscribers.
    ///
    /// Fire-and-forget: if there are no subscribers, the resulting send
    /// error is intentionally ignored (there is nothing useful to do about
    /// it).
    pub fn send(&self, event: T) {
        let _ = self.sender.send(event);
    }

    /// Number of currently-subscribed receivers.
    #[must_use]
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Subscribe to the raw (un-encoded) event stream, with no Axum/SSE
    /// wrapping. Useful for unit tests and for any non-HTTP consumer that
    /// just wants the plain `T` values.
    pub fn subscribe_events(&self) -> impl Stream<Item = T> + Send + 'static {
        self.raw_stream(None)
    }

    /// Subscribe to the live event stream only (no hydration event).
    pub fn subscribe(
        self: Arc<Self>,
        opts: SseOptions,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
        Self::to_sse(self.raw_stream(None), opts)
    }

    /// Subscribe, first emitting one synthetic `initial` event (e.g. a full
    /// current-state snapshot) before streaming live events.
    ///
    /// This gives new subscribers the current state immediately, instead of
    /// waiting for the next broadcast - the "subscribe-first-then-hydrate"
    /// pattern used by this codebase's SSE consumers.
    pub fn subscribe_with_hydration(
        self: Arc<Self>,
        initial: T,
        opts: SseOptions,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
        Self::to_sse(self.raw_stream(Some(initial)), opts)
    }

    /// Raw, unencoded event stream: optionally prefixed with one `initial`
    /// event, then the live broadcast stream with lagged/closed receivers
    /// silently skipped. Kept separate from [`Self::subscribe`] so the
    /// hydration-ordering and lag-handling behavior can be unit tested
    /// without going through Axum's SSE/`Event` types.
    fn raw_stream(&self, initial: Option<T>) -> impl Stream<Item = T> + Send + 'static + use<T> {
        let receiver = self.sender.subscribe();
        let live = BroadcastStream::new(receiver).filter_map(|result| match result {
            Ok(event) => Some(event),
            Err(e) => {
                tracing::debug!("SSE stream error: {e}");
                None
            }
        });
        stream::iter(initial).chain(live)
    }

    fn to_sse(
        events: impl Stream<Item = T> + Send + 'static,
        opts: SseOptions,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
        let encoded = events.filter_map(|event| Self::encode(&event));
        Sse::new(encoded).keep_alive(
            KeepAlive::new()
                .interval(opts.keepalive_interval)
                .text(opts.keepalive_text),
        )
    }

    fn encode(event: &T) -> Option<Result<Event, Infallible>> {
        match serde_json::to_string(event) {
            Ok(json) => Some(Ok(Event::default().data(json))),
            Err(e) => {
                tracing::warn!("Failed to serialize SSE event: {e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    struct TestEvent(u32);

    #[tokio::test]
    async fn subscribe_receives_published_events() {
        let broadcaster = Broadcaster::<TestEvent>::new(8);
        let mut stream = broadcaster.raw_stream(None);
        broadcaster.send(TestEvent(1));

        assert_eq!(stream.next().await, Some(TestEvent(1)));
    }

    #[tokio::test]
    async fn hydration_event_arrives_before_live_events() {
        let broadcaster = Broadcaster::<TestEvent>::new(8);
        let mut stream = broadcaster.raw_stream(Some(TestEvent(0)));
        broadcaster.send(TestEvent(1));

        assert_eq!(stream.next().await, Some(TestEvent(0)));
        assert_eq!(stream.next().await, Some(TestEvent(1)));
    }

    #[tokio::test]
    async fn lagging_subscriber_skips_missed_events_without_panicking() {
        let broadcaster = Broadcaster::<TestEvent>::new(2);
        let mut stream = broadcaster.raw_stream(None);

        for i in 0..10 {
            broadcaster.send(TestEvent(i));
        }

        // Must not panic even though far more events were sent than the
        // channel capacity - the lagged receiver error is skipped and the
        // stream resumes with whatever survived the ring buffer.
        let received = stream.next().await;
        assert!(received.is_some());
    }

    #[tokio::test]
    async fn subscriber_count_reflects_active_subscriptions() {
        let broadcaster = Arc::new(Broadcaster::<TestEvent>::new(8));
        assert_eq!(broadcaster.subscriber_count(), 0);

        let _stream = broadcaster.raw_stream(None);
        assert_eq!(broadcaster.subscriber_count(), 1);
    }
}
