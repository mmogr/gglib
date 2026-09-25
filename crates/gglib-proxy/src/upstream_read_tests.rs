//! Tests for [`super`]: the upstream reply read into events, and the
//! first-byte timeout body.

use std::cell::Cell;

use super::*;

/// One SSE frame whose delta is the content `text`.
fn content_frame(text: &str) -> Bytes {
    Bytes::from(format!(
        "data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{text}\"}},\"finish_reason\":null}}]}}\n\n"
    ))
}

/// The events [`upstream_events`] yields for `chunks`, and how many chunks it
/// read to get them.
async fn read_all(
    chunks: Vec<Result<Bytes, std::io::Error>>,
) -> (Vec<anyhow::Result<LlmStreamEvent>>, usize) {
    let reads = Cell::new(0);
    let bytes = futures_util::stream::iter(chunks).inspect(|_| reads.set(reads.get() + 1));
    let events = upstream_events(bytes).collect().await;
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
    let body = first_byte_timeout_frame("some-model");
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
