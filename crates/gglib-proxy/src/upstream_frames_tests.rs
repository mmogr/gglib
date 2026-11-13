//! Tests for [`super::upstream_frames`]: the upstream reply read into items
//! under the idle bound.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::StreamExt as _;
use gglib_core::LlmStreamEvent;
use gglib_core::sse::SseItem;

use super::{UpstreamStalled, upstream_frames};

/// The idle bound the timing tests read under.
const IDLE: Duration = Duration::from_millis(200);

/// One SSE frame whose delta is the content `text`.
fn content_frame(text: &str) -> Bytes {
    Bytes::from(format!(
        "data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{text}\"}},\"finish_reason\":null}}]}}\n\n"
    ))
}

/// The items [`upstream_frames`] yields for `chunks`, and how many chunks it
/// read to get them.
async fn read_all(
    chunks: Vec<Result<Bytes, std::io::Error>>,
) -> (Vec<anyhow::Result<SseItem>>, usize) {
    let reads = AtomicUsize::new(0);
    let bytes = futures_util::stream::iter(chunks).inspect(|_| {
        reads.fetch_add(1, Ordering::Relaxed);
    });
    let items = upstream_frames(bytes, IDLE).collect().await;
    (items, reads.load(Ordering::Relaxed))
}

/// The events of every item, asserting none is an error, with `[DONE]` as
/// `None`; a comment gives nothing.
fn events(items: Vec<anyhow::Result<SseItem>>) -> Vec<Option<LlmStreamEvent>> {
    items
        .into_iter()
        .flat_map(|item| match item.expect("no item is an error") {
            SseItem::Data { events, .. } => events.into_iter().map(Some).collect(),
            SseItem::Done => vec![None],
            SseItem::Comment(_) => vec![],
        })
        .collect()
}

fn text(content: &str) -> LlmStreamEvent {
    LlmStreamEvent::TextDelta {
        content: content.to_owned(),
    }
}

/// A transport error mid-reply ends the items with that error, after the
/// items already read, and no further chunk is read.
#[tokio::test]
async fn a_byte_stream_that_breaks_ends_in_an_error_after_its_items() {
    let chunks = vec![
        Ok(content_frame("hel")),
        Err(std::io::Error::other("connection reset")),
        Ok(content_frame("lo")),
    ];
    let (mut items, reads) = read_all(chunks).await;

    assert_eq!(reads, 2, "the chunk after the error is never read");
    assert_eq!(items.len(), 2, "one frame, then the error: {items:?}");
    let error = items.pop().unwrap().expect_err("the break is an error");
    assert_eq!(
        error.to_string(),
        "upstream SSE byte-stream error: connection reset"
    );
    assert_eq!(events(items), vec![Some(text("hel"))]);
}

/// Reading stops at `[DONE]`, which is the last item. The chunk after it is
/// never read, so its text never reaches the client.
#[tokio::test]
async fn reading_stops_at_the_done_sentinel_and_the_next_chunk_is_never_read() {
    let mut first = content_frame("hel").to_vec();
    first.extend_from_slice(b"data: [DONE]\n\n");
    let chunks = vec![Ok(Bytes::from(first)), Ok(content_frame("lo"))];
    let (items, reads) = read_all(chunks).await;

    assert_eq!(reads, 1, "the chunk after [DONE] is never read");
    assert_eq!(events(items), vec![Some(text("hel")), None]);
}

/// A body that just ends, with no `[DONE]` and no finish reason, ends with
/// its last frame: nothing is added for a finish nobody sent.
#[tokio::test]
async fn a_body_that_just_ends_ends_with_its_last_frame() {
    let (items, reads) = read_all(vec![Ok(content_frame("hel"))]).await;

    assert_eq!(reads, 1);
    assert_eq!(events(items), vec![Some(text("hel"))]);
}

/// Every item of `items`, failing the test rather than hanging it if they
/// never end.
async fn within_patience<S>(items: S) -> Vec<anyhow::Result<SseItem>>
where
    S: futures_util::Stream<Item = anyhow::Result<SseItem>>,
{
    tokio::time::timeout(IDLE * 30, items.collect())
        .await
        .expect("the items end")
}

/// The stall error a read's items ended with, if they ended in one.
fn stall_of(item: &anyhow::Result<SseItem>) -> Option<UpstreamStalled> {
    item.as_ref()
        .err()?
        .downcast_ref::<UpstreamStalled>()
        .copied()
}

/// An upstream that sends each of `chunks` `gap` after it is asked for the
/// next, then says nothing more.
fn then_silent(
    chunks: Vec<Bytes>,
    gap: Duration,
) -> impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> {
    futures_util::stream::iter(chunks)
        .then(move |chunk| async move {
            tokio::time::sleep(gap).await;
            Ok(chunk)
        })
        .chain(futures_util::stream::pending())
}

/// A read that waits past the bound ends the items in a stall, after what
/// had already arrived, and no sooner than the bound.
#[tokio::test]
async fn a_reply_that_goes_silent_past_the_bound_ends_in_a_stall() {
    let started = Instant::now();
    let silent = then_silent(vec![content_frame("hel")], Duration::ZERO);
    let items = within_patience(upstream_frames(silent, IDLE)).await;

    assert!(started.elapsed() >= IDLE);
    assert_eq!(items.len(), 2, "{items:?}");
    assert!(matches!(items[0], Ok(SseItem::Data { .. })));
    assert_eq!(
        stall_of(&items[1]),
        Some(UpstreamStalled {
            after: IDLE,
            after_first_token: true,
        })
    );
}

/// Chunks that keep arriving inside the bound are never cut, however long
/// the reply runs in all.
#[tokio::test]
async fn bytes_that_keep_arriving_inside_the_bound_are_never_cut() {
    let mut chunks: Vec<_> = (0..6).map(|i| content_frame(&i.to_string())).collect();
    chunks.push(Bytes::from_static(b"data: [DONE]\n\n"));
    let started = Instant::now();
    let items = within_patience(upstream_frames(then_silent(chunks, IDLE / 2), IDLE)).await;

    assert!(started.elapsed() > IDLE * 2);
    assert_eq!(events(items).len(), 7, "six texts and the [DONE]");
}

/// The timer runs while a read waits and at no other time: a consumer that
/// takes longer than the bound to ask for the next item does not make the
/// upstream look silent, even though the next chunk then takes half the bound
/// to come.
#[tokio::test]
async fn the_bound_does_not_run_while_nobody_asks_for_the_next_chunk() {
    let chunks = vec![content_frame("hel"), content_frame("lo")];
    let items = upstream_frames(then_silent(chunks, IDLE / 2), IDLE);
    let mut items = std::pin::pin!(items);

    assert!(matches!(items.next().await, Some(Ok(SseItem::Data { .. }))));
    tokio::time::sleep(IDLE * 2).await;
    let second = items.next().await.expect("a second item");
    let SseItem::Data { events, .. } = second.expect("the second chunk, not a stall") else {
        panic!("a frame");
    };
    assert_eq!(events, vec![text("lo")]);
}

/// llama-server's keep-alive comment, which it sends after each 30 s it has
/// had nothing else to send (`--sse-ping-interval`).
const PING: &[u8] = b":\n\n";

/// An upstream that answers every read with `chunk`, `gap` after it is asked.
fn every(
    chunk: Bytes,
    gap: Duration,
) -> impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> {
    futures_util::stream::repeat(chunk).then(move |chunk| async move {
        tokio::time::sleep(gap).await;
        Ok(chunk)
    })
}

/// Once tokens flow, a keep-alive comment is not the reply moving: an
/// upstream that sends nothing else, however often, still ends in a stall.
/// Each comment is still an item, for the client.
#[tokio::test]
async fn keep_alive_comments_alone_still_end_in_a_stall() {
    let started = Instant::now();
    let first = futures_util::stream::once(async { Ok(content_frame("a")) });
    let pings = first.chain(every(Bytes::from_static(PING), IDLE / 4));
    let items = within_patience(upstream_frames(pings, IDLE)).await;

    assert!(started.elapsed() >= IDLE);
    let (last, before) = items.split_last().expect("items");
    assert_eq!(
        stall_of(last),
        Some(UpstreamStalled {
            after: IDLE,
            after_first_token: true,
        })
    );
    assert!(matches!(before[0], Ok(SseItem::Data { .. })));
    assert!(before.len() > 1, "the pings came through: {before:?}");
    assert!(
        before[1..]
            .iter()
            .all(|item| matches!(item, Ok(SseItem::Comment(c)) if c == ":"))
    );
}

/// Before the first token they are: between two prefill-progress chunks
/// llama-server sends only these, so a slow prefill is never cut.
#[tokio::test]
async fn keep_alive_comments_carry_a_slow_prefill() {
    let mut chunks = vec![Bytes::from_static(PING); 12];
    chunks.push(content_frame("a"));
    chunks.push(Bytes::from_static(b"data: [DONE]\n\n"));
    let started = Instant::now();
    let items = within_patience(upstream_frames(then_silent(chunks, IDLE / 4), IDLE)).await;

    assert!(started.elapsed() > IDLE * 2);
    assert_eq!(events(items), vec![Some(text("a")), None]);
}

/// A prefill progress frame as llama-server b12110 writes one: the role
/// delta with `prompt_progress` beside it.
const PROGRESS: &[u8] = b"data: {\"choices\":[{\"finish_reason\":null,\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null}}],\"prompt_progress\":{\"cache\":0,\"processed\":1,\"total\":9,\"time_ms\":1}}\n\n";

/// Comments between frames that come inside the bound cut nothing: a chunk
/// with a frame in it, prefill progress or text, a comment in it or not,
/// restores the whole bound.
#[tokio::test]
async fn keep_alive_comments_between_timely_frames_are_never_cut() {
    let mut chunks = Vec::new();
    for i in 0..6 {
        let frame = if i < 3 {
            Bytes::from_static(PROGRESS)
        } else {
            content_frame(&i.to_string())
        };
        chunks.push(Bytes::from_static(PING));
        chunks.push(Bytes::from([PING, &frame].concat()));
    }
    chunks.push(Bytes::from_static(b"data: [DONE]\n\n"));
    let started = Instant::now();
    let items = within_patience(upstream_frames(then_silent(chunks, IDLE / 4), IDLE)).await;

    assert!(started.elapsed() > IDLE * 2);
    assert_eq!(
        events(items).len(),
        7,
        "three progress frames, three texts and the [DONE]"
    );
}

/// Silence after prefill progress alone is a stall before the first token.
#[tokio::test]
async fn a_stall_before_any_generated_token_says_so() {
    let silent = then_silent(vec![Bytes::from_static(PROGRESS)], Duration::ZERO);
    let items = within_patience(upstream_frames(silent, IDLE)).await;

    let stall = items.last().and_then(stall_of).expect("ends in a stall");
    assert!(!stall.after_first_token);
}
