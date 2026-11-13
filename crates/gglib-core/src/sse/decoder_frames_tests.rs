//! [`SseStreamDecoder::feed_frames`]: each line that says something, as an
//! item, in order, with nothing added.

use serde_json::json;

use super::{SseItem, SseStreamDecoder};
use crate::LlmStreamEvent;

/// Every item `wire` gives in one feed, asserting none is an error.
fn items(wire: &str) -> (Vec<SseItem>, bool) {
    let (items, stop) = SseStreamDecoder::default().feed_frames(wire.as_bytes());
    let items = items
        .into_iter()
        .map(|item| item.expect("no item is an error"))
        .collect();
    (items, stop)
}

const ROLE: &str = r#"{"choices":[{"finish_reason":null,"index":0,"delta":{"role":"assistant","content":null}}],"id":"c"}"#;
const TEXT: &str =
    r#"{"choices":[{"finish_reason":null,"index":0,"delta":{"content":"hi"}}],"id":"c"}"#;
const FINISH: &str = r#"{"choices":[{"finish_reason":"stop","index":0,"delta":{}}],"id":"c"}"#;

#[test]
fn comments_frames_and_done_come_out_in_order_with_each_payload_as_sent() {
    let wire = format!(
        ": ping\n\ndata: {ROLE}\n\n:\n\ndata: {TEXT}\n\ndata: {FINISH}\n\ndata: [DONE]\n\n"
    );
    let (items, stop) = items(&wire);

    assert!(stop, "[DONE] ends the stream");
    let [
        SseItem::Comment(ping),
        SseItem::Data {
            text: role,
            events: role_events,
            ..
        },
        SseItem::Comment(bare),
        SseItem::Data {
            text,
            value,
            events,
        },
        SseItem::Data { events: finish, .. },
        SseItem::Done,
    ] = items.as_slice()
    else {
        panic!("six items: {items:?}");
    };
    assert_eq!((ping.as_str(), bare.as_str()), (": ping", ":"));
    assert_eq!(role, ROLE, "the payload as it arrived, key order and all");
    assert!(role_events.is_empty(), "a role frame carries no event");
    assert_eq!(text, TEXT);
    assert_eq!(value["choices"][0]["delta"]["content"], json!("hi"));
    assert_eq!(
        events,
        &[LlmStreamEvent::TextDelta {
            content: "hi".to_owned()
        }]
    );
    assert_eq!(
        finish,
        &[LlmStreamEvent::Done {
            finish_reason: Some("stop".to_owned())
        }]
    );
}

/// `[DONE]` with no finish before it is only `[DONE]`: the fallback `Done`
/// is `feed_bytes`'s, not a frame's.
#[test]
fn done_without_a_finish_adds_no_event() {
    let (items, stop) = items(&format!("data: {TEXT}\n\ndata: [DONE]\n\n"));

    assert!(stop);
    assert_eq!(items.len(), 2);
    assert_eq!(items[1], SseItem::Done);
}

#[test]
fn a_body_that_just_ends_ends_with_its_last_frame() {
    let (items, stop) = items(&format!("data: {TEXT}\n\n"));

    assert!(!stop);
    assert_eq!(items.len(), 1);
}

/// An inline error ends the stream with its frame; the line after it is
/// never read.
#[test]
fn an_error_frame_is_the_last_item() {
    let wire = format!(
        "data: {{\"error\":{{\"code\":500,\"message\":\"gone\",\"type\":\"server_error\"}}}}\n\ndata: {TEXT}\n\n"
    );
    let (items, stop) = items(&wire);

    assert!(stop);
    let [SseItem::Data { value, events, .. }] = items.as_slice() else {
        panic!("one item: {items:?}");
    };
    assert_eq!(value["error"]["code"], json!(500), "the frame as sent");
    assert_eq!(
        events,
        &[LlmStreamEvent::UpstreamError {
            message: "gone".to_owned(),
            error_type: "server_error".to_owned(),
            code: "upstream_error".to_owned(),
        }]
    );
}

#[test]
fn a_payload_that_is_not_json_is_an_error_and_ends_the_stream() {
    let wire = format!("data: {TEXT}\n\ndata: {{nope\n\ndata: {TEXT}\n\n");
    let (items, stop) = SseStreamDecoder::default().feed_frames(wire.as_bytes());

    assert!(stop);
    assert_eq!(items.len(), 2);
    assert!(items[0].is_ok());
    assert!(items[1].is_err());
}

/// The same bytes give the same items however they are cut.
#[test]
fn a_frame_cut_across_chunks_is_read_whole() {
    let wire = format!(": ping\n\ndata: {TEXT}\n\ndata: [DONE]\n\n");
    let whole = items(&wire).0;
    for at in 1..wire.len() {
        let mut decoder = SseStreamDecoder::default();
        let (mut got, stop) = decoder.feed_frames(&wire.as_bytes()[..at]);
        if !stop {
            got.extend(decoder.feed_frames(&wire.as_bytes()[at..]).0);
        }
        let got: Vec<SseItem> = got.into_iter().map(|i| i.expect("ok")).collect();
        assert_eq!(got, whole, "cut at {at}");
    }
}
