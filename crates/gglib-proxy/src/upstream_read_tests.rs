//! Tests for [`super`]: the upstream reply read into events under the idle
//! bound, and the `upstream_timeout` bodies.

use std::cell::Cell;
use std::time::Instant;

use super::*;

/// The idle bound the timing tests read under.
const IDLE: Duration = Duration::from_millis(200);

/// One SSE frame whose delta is the content `text`.
fn content_frame(text: &str) -> Bytes {
    Bytes::from(format!(
        "data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{text}\"}},\"finish_reason\":null}}]}}\n\n"
    ))
}

/// The events [`upstream_events`] yields for `chunks`, and how many chunks it
/// read to get them.
#[allow(
    clippy::future_not_send,
    reason = "grandfathered at lint inheritance, #1157"
)]
async fn read_all(
    chunks: Vec<Result<Bytes, std::io::Error>>,
) -> (Vec<anyhow::Result<LlmStreamEvent>>, usize) {
    let reads = Cell::new(0);
    let bytes = futures_util::stream::iter(chunks).inspect(|_| reads.set(reads.get() + 1));
    let events = upstream_events(bytes, IDLE).collect().await;
    (events, reads.get())
}

/// Every event, asserting none is an error.
fn all_ok(events: Vec<anyhow::Result<LlmStreamEvent>>) -> Vec<LlmStreamEvent> {
    events
        .into_iter()
        .map(|event| event.expect("no event is an error"))
        .collect()
}

/// Replace the value after `key` (the run of characters `keep` accepts) with
/// `mask`, asserting there is exactly one such value and that it is non-empty.
fn mask_one(body: &str, key: &str, keep: fn(char) -> bool, mask: &str) -> String {
    assert_eq!(body.matches(key).count(), 1, "expected one {key} in {body}");
    let start = body.find(key).expect("counted above") + key.len();
    let len: usize = body[start..]
        .chars()
        .take_while(|&c| keep(c))
        .map(char::len_utf8)
        .sum();
    assert!(len > 0, "{key} has no value in {body}");
    format!("{}{mask}{}", &body[..start], &body[start + len..])
}

/// The first-byte timeout body, byte for byte, except the chunk id, which is
/// random, and the `created` timestamp, which follows the clock. ggchat's
/// `ErrorTests` quote this error (code `upstream_timeout`, message "upstream
/// did not respond within 300s") and read it as "wait and retry"; any change
/// to these bytes is a change to what clients are sent.
#[test]
fn the_first_byte_timeout_body_is_a_notice_then_upstream_timeout_then_done() {
    let body = first_byte_timeout_frame("some-model", StreamBounds::default().first_byte);
    let body = mask_one(&body, "\"id\":\"chatcmpl-", |c| c.is_ascii_hexdigit(), "ID");
    let body = mask_one(&body, "\"created\":", |c| c.is_ascii_digit(), "0");
    assert_eq!(body, EXPECTED_FIRST_BYTE_TIMEOUT_BODY);
}

const EXPECTED_FIRST_BYTE_TIMEOUT_BODY: &str = concat!(
    r#"data: {"choices":[{"delta":{"content":"#,
    r#""⚠️ [proxy] upstream model server did not begin responding within 300s"#,
    r#" — it may be overloaded or wedged."#,
    r#" Retry; if it persists the model will be recycled."#,
    r#""},"finish_reason":null,"index":0}],"#,
    r#""created":0,"id":"chatcmpl-ID","#,
    r#""model":"some-model","object":"chat.completion.chunk"}"#,
    "\n\n",
    r#"data: {"error":{"code":"upstream_timeout","#,
    r#""message":"upstream did not respond within 300s","#,
    r#""type":"server_error"}}"#,
    "\n\n",
    "data: [DONE]\n\n",
);

/// A transport error mid-reply ends the events with that error, after the
/// events already decoded, and nothing follows: no fallback `Done`, and no
/// further chunk is read.
#[tokio::test]
async fn a_byte_stream_that_breaks_ends_in_an_error_after_its_events() {
    let chunks = vec![
        Ok(content_frame("hel")),
        Err(std::io::Error::other("connection reset")),
        Ok(content_frame("lo")),
    ];
    let (events, reads) = read_all(chunks).await;

    assert_eq!(reads, 2, "the chunk after the error is never read");
    assert_eq!(events.len(), 2, "one event, then the error: {events:?}");
    assert_eq!(
        events[0].as_ref().expect("the first chunk decodes"),
        &LlmStreamEvent::TextDelta {
            content: "hel".to_owned()
        }
    );
    let error = events[1].as_ref().expect_err("the break is an error");
    assert_eq!(
        error.to_string(),
        "upstream SSE byte-stream error: connection reset"
    );
}

/// Reading stops where the decoder says the stream ended: here at `[DONE]`,
/// which yields the turn's one `Done`. The chunk after it is never read, so
/// its text never reaches the client.
#[tokio::test]
async fn reading_stops_at_the_done_sentinel_and_the_next_chunk_is_never_read() {
    let mut first = content_frame("hel").to_vec();
    first.extend_from_slice(b"data: [DONE]\n\n");
    let chunks = vec![Ok(Bytes::from(first)), Ok(content_frame("lo"))];
    let (events, reads) = read_all(chunks).await;

    assert_eq!(reads, 1, "the chunk after [DONE] is never read");
    assert_eq!(
        all_ok(events),
        vec![
            LlmStreamEvent::TextDelta {
                content: "hel".to_owned()
            },
            LlmStreamEvent::Done {
                finish_reason: None
            },
        ]
    );
}

/// A body that just ends, with no `[DONE]` and no finish reason, still ends
/// in one `Done`: the decoder's fallback. It carries no finish reason, since
/// nothing upstream said the turn finished. The normalizer's contract is that
/// every stream carries exactly one `Done`.
#[tokio::test]
async fn a_body_that_just_ends_gets_a_fallback_done_with_no_finish_reason() {
    let (events, reads) = read_all(vec![Ok(content_frame("hel"))]).await;

    assert_eq!(reads, 1);
    assert_eq!(
        all_ok(events),
        vec![
            LlmStreamEvent::TextDelta {
                content: "hel".to_owned()
            },
            LlmStreamEvent::Done {
                finish_reason: None
            },
        ]
    );
}

/// Every event of `events`, failing the test rather than hanging it if they
/// never end.
async fn within_patience<S>(events: S) -> Vec<anyhow::Result<LlmStreamEvent>>
where
    S: futures_util::Stream<Item = anyhow::Result<LlmStreamEvent>>,
{
    tokio::time::timeout(IDLE * 30, events.collect())
        .await
        .expect("the events end")
}

/// The stall error a read's events ended with, if they ended in one.
fn stall_of(event: &anyhow::Result<LlmStreamEvent>) -> Option<UpstreamStalled> {
    event
        .as_ref()
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

/// A read that waits past the bound ends the events in a stall, after what
/// had already arrived, and no sooner than the bound.
#[tokio::test]
async fn a_reply_that_goes_silent_past_the_bound_ends_in_a_stall() {
    let started = Instant::now();
    let silent = then_silent(vec![content_frame("hel")], Duration::ZERO);
    let events = within_patience(upstream_events(silent, IDLE)).await;

    assert!(started.elapsed() >= IDLE);
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(matches!(events[0], Ok(LlmStreamEvent::TextDelta { .. })));
    assert_eq!(
        stall_of(&events[1]),
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
    let events = within_patience(upstream_events(then_silent(chunks, IDLE / 2), IDLE)).await;

    assert!(started.elapsed() > IDLE * 2);
    assert_eq!(all_ok(events).len(), 7, "six texts and the Done");
}

/// The timer runs while a read waits and at no other time: a consumer that
/// takes longer than the bound to ask for the next event does not make the
/// upstream look silent, even though the next chunk then takes half the bound
/// to come.
#[tokio::test]
async fn the_bound_does_not_run_while_nobody_asks_for_the_next_chunk() {
    let chunks = vec![content_frame("hel"), content_frame("lo")];
    let events = upstream_events(then_silent(chunks, IDLE / 2), IDLE);
    let mut events = std::pin::pin!(events);

    assert!(matches!(
        events.next().await,
        Some(Ok(LlmStreamEvent::TextDelta { .. }))
    ));
    tokio::time::sleep(IDLE * 2).await;
    let second = events.next().await.expect("a second event");
    assert_eq!(
        second.expect("the second chunk, not a stall"),
        LlmStreamEvent::TextDelta {
            content: "lo".to_owned()
        }
    );
}

/// Silence after prefill progress alone is a stall before the first token.
#[tokio::test]
async fn a_stall_before_any_generated_token_says_so() {
    let progress = Bytes::from_static(
        b"data: {\"prompt_progress\":{\"cache\":0,\"processed\":1,\"total\":9,\"time_ms\":1}}\n\n",
    );
    let silent = then_silent(vec![progress], Duration::ZERO);
    let events = within_patience(upstream_events(silent, IDLE)).await;

    let stall = events.last().and_then(stall_of).expect("ends in a stall");
    assert!(!stall.after_first_token);
}

/// What the client is sent for a stall: the notice, which promises a recycle
/// only when there will be one, and the error frame, byte for byte.
#[test]
fn a_stall_is_told_as_a_notice_and_an_upstream_timeout_frame() {
    let after = Duration::from_mins(5);
    let mid = UpstreamStalled {
        after,
        after_first_token: true,
    };
    let early = UpstreamStalled {
        after,
        after_first_token: false,
    };

    assert_eq!(
        mid.notice(),
        "\n\n⚠️ [proxy] upstream model server went silent for 300s mid-response — it may be wedged; this model is being recycled."
    );
    assert_eq!(
        early.notice(),
        "\n\n⚠️ [proxy] upstream model server went silent for 300s mid-response — it may be wedged."
    );
    assert_eq!(
        mid.error_frame(),
        concat!(
            r#"data: {"error":{"code":"upstream_timeout","#,
            r#""message":"upstream sent nothing for 300s mid-response","#,
            r#""type":"server_error"}}"#,
            "\n\n",
        )
    );
}
