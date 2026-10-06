//! A chunk boundary is the network's, not the stream's: wherever one falls,
//! the decoder yields what it yields for the same bytes fed whole.

use super::SseStreamDecoder;
use crate::LlmStreamEvent;

/// One item the decoder yields, an error as its text.
type Yielded = Result<LlmStreamEvent, String>;

/// Everything a caller sees of one stream.
#[derive(Debug, PartialEq, Eq)]
struct Decoded {
    events: Vec<Yielded>,
    stopped: bool,
    fallback: Option<LlmStreamEvent>,
}

/// Feed `chunks` in order, stopping when the decoder says to, as a caller
/// does.
fn decode(chunks: &[&[u8]]) -> Decoded {
    let mut decoder = SseStreamDecoder::default();
    let mut events = Vec::new();
    let mut stopped = false;
    for chunk in chunks {
        let (fed, stop) = decoder.feed_bytes(chunk);
        events.extend(
            fed.into_iter()
                .map(|event| event.map_err(|e| e.to_string())),
        );
        if stop {
            stopped = true;
            break;
        }
    }
    Decoded {
        events,
        stopped,
        fallback: decoder.finish(),
    }
}

fn text(content: &str) -> LlmStreamEvent {
    LlmStreamEvent::TextDelta {
        content: content.to_owned(),
    }
}

fn done(reason: Option<&str>) -> LlmStreamEvent {
    LlmStreamEvent::Done {
        finish_reason: reason.map(str::to_owned),
    }
}

/// One content frame, as llama-server writes it.
fn content_frame(content: &str) -> String {
    format!(
        "data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":\"{content}\"}},\"finish_reason\":null}}]}}\n\n"
    )
}

/// `wire` cut in two at each of `cuts`, checked against `wire` fed whole.
fn assert_cuts_decode_as_whole(wire: &[u8], cuts: impl IntoIterator<Item = usize>) -> Decoded {
    let whole = decode(&[wire]);
    for cut in cuts {
        assert_eq!(
            decode(&[&wire[..cut], &wire[cut..]]),
            whole,
            "cut at byte {cut}"
        );
    }
    whole
}

#[test]
fn a_four_byte_emoji_split_across_two_chunks_is_one_character() {
    let wire = format!("{}data: [DONE]\n\n", content_frame("go \u{1f680} now"));
    let at = wire.find('\u{1f680}').expect("the emoji is on the wire");
    assert_eq!(&wire.as_bytes()[at..at + 4], [0xf0, 0x9f, 0x9a, 0x80]);

    let whole = assert_cuts_decode_as_whole(wire.as_bytes(), [at + 1, at + 2, at + 3]);

    assert_eq!(whole.events, [Ok(text("go \u{1f680} now")), Ok(done(None))]);
    assert!(whole.stopped);
}

#[test]
fn a_three_byte_cjk_character_split_across_two_chunks_is_one_character() {
    let wire = format!("{}data: [DONE]\n\n", content_frame("\u{6771}\u{4eac}"));
    let at = wire.find('\u{6771}').expect("the character is on the wire");
    assert_eq!(&wire.as_bytes()[at..at + 3], [0xe6, 0x9d, 0xb1]);

    let whole = assert_cuts_decode_as_whole(wire.as_bytes(), [at + 1, at + 2]);

    assert_eq!(whole.events, [Ok(text("\u{6771}\u{4eac}")), Ok(done(None))]);
}

#[test]
fn the_first_half_of_a_character_is_held_not_refused() {
    let wire = content_frame("\u{1f680}");
    let at = wire.find('\u{1f680}').expect("the emoji is on the wire");
    let mut decoder = SseStreamDecoder::default();

    let (events, stop) = decoder.feed_bytes(&wire.as_bytes()[..at + 2]);

    assert!(events.is_empty(), "nothing is decoded before its line ends");
    assert!(!stop);
}

/// A reply as llama-server streams it: prefill progress, the role chunk,
/// reasoning, content, a tool call, the finish chunk carrying usage, and
/// the sentinel.
const REPLY: &str = concat!(
    ": a comment is not an event\n\n",
    r#"data: {"prompt_progress":{"total":12,"cache":4,"processed":12,"time_ms":3}}"#,
    "\n\n",
    r#"data: {"choices":[{"index":0,"delta":{"role":"assistant","content":null},"finish_reason":null}]}"#,
    "\n\n",
    r#"data: {"choices":[{"index":0,"delta":{"reasoning_content":"考える"},"finish_reason":null}]}"#,
    "\n\n",
    r#"data: {"choices":[{"index":0,"delta":{"content":"Voilà 🚀"},"finish_reason":null}]}"#,
    "\n\n",
    r#"data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"search","arguments":"{\"q\":\"東京\"}"}}]},"finish_reason":null}]}"#,
    "\n\n",
    r#"data: {"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":12,"completion_tokens":7,"total_tokens":19}}"#,
    "\n\n",
    "data: [DONE]\n\n",
);

/// What [`REPLY`] decodes to. The comment and the role chunk yield nothing.
fn reply_events() -> Vec<Yielded> {
    vec![
        Ok(LlmStreamEvent::PromptProgress {
            processed: 12,
            total: 12,
            cached: 4,
            time_ms: 3,
        }),
        Ok(LlmStreamEvent::ReasoningDelta {
            content: "考える".to_owned(),
        }),
        Ok(text("Voilà 🚀")),
        Ok(LlmStreamEvent::ToolCallDelta {
            index: 0,
            id: Some("call_1".to_owned()),
            name: Some("search".to_owned()),
            arguments: Some(r#"{"q":"東京"}"#.to_owned()),
        }),
        Ok(LlmStreamEvent::Usage {
            prompt_tokens: 12,
            completion_tokens: 7,
            total_tokens: 19,
            cached_tokens: None,
        }),
        Ok(done(Some("tool_calls"))),
    ]
}

#[test]
fn a_reply_cut_at_every_byte_offset_decodes_as_it_does_whole() {
    let wire = REPLY.as_bytes();

    let whole = assert_cuts_decode_as_whole(wire, 1..wire.len());

    assert_eq!(whole.events, reply_events());
    assert!(whole.stopped);
    assert_eq!(whole.fallback, None);
}

#[test]
fn a_reply_fed_one_byte_at_a_time_decodes_as_it_does_whole() {
    let bytes: Vec<&[u8]> = REPLY.as_bytes().chunks(1).collect();

    assert_eq!(decode(&bytes).events, reply_events());
}

#[test]
fn crlf_line_endings_decode_as_lf_ones_do_wherever_the_chunk_ends() {
    let wire = REPLY.replace('\n', "\r\n");
    let wire = wire.as_bytes();

    let whole = assert_cuts_decode_as_whole(wire, 1..wire.len());

    assert_eq!(whole.events, reply_events());
    assert!(whole.stopped);
}

#[test]
fn a_chunk_that_ends_between_cr_and_lf_ends_the_line_when_the_lf_arrives() {
    let wire = b"data: [DONE]\r\n\r\n";
    let cr = wire.iter().position(|&byte| byte == b'\r').expect("a \\r");
    let mut decoder = SseStreamDecoder::default();

    let (before, stopped_early) = decoder.feed_bytes(&wire[..=cr]);
    let (after, stop) = decoder.feed_bytes(&wire[cr + 1..]);

    assert!(before.is_empty(), "the line has not ended at its \\r");
    assert!(!stopped_early);
    let after: Vec<Yielded> = after
        .into_iter()
        .map(|event| event.map_err(|e| e.to_string()))
        .collect();
    assert_eq!(after, [Ok(done(None))], "the \\r is no part of the line");
    assert!(stop);
}

#[test]
fn a_complete_line_that_is_not_utf8_ends_the_stream_with_an_error() {
    let mut wire = content_frame("before").into_bytes();
    wire.extend_from_slice(b"data: {\"choices\":[{\"delta\":{\"content\":\"\xff\"}}]}\n\n");
    wire.extend_from_slice(content_frame("after").as_bytes());

    let got = decode(&[&wire]);

    assert_eq!(got.events.len(), 2, "nothing after the error: {got:?}");
    assert_eq!(got.events[0], Ok(text("before")));
    let error = got.events[1]
        .as_ref()
        .expect_err("the bad line is an error");
    assert!(error.contains("invalid UTF-8"), "{error}");
    assert!(got.stopped);
}

#[test]
fn a_character_cut_short_by_its_line_ending_is_an_error_not_a_guess() {
    let whole = content_frame("\u{1f680}");
    let at = whole.find('\u{1f680}').expect("the emoji is on the wire");
    let mut wire = whole.as_bytes()[..at + 2].to_vec();
    wire.extend_from_slice(b"\"}}]}\n\n");

    let got = decode(&[&wire]);

    assert_eq!(got.events.len(), 1, "{got:?}");
    let error = got.events[0].as_ref().expect_err("half a character");
    assert!(error.contains("invalid UTF-8"), "{error}");
    assert!(got.stopped);
}

#[test]
fn done_stops_the_stream_and_nothing_after_it_is_read() {
    let mut wire = content_frame("answer").into_bytes();
    wire.extend_from_slice(b"data: [DONE]\n\n");
    wire.extend_from_slice(content_frame("late").as_bytes());
    wire.extend_from_slice(b"data: not json\n\ndata: \xff\n\n");

    let got = decode(&[&wire]);

    assert_eq!(got.events, [Ok(text("answer")), Ok(done(None))]);
    assert!(got.stopped);
    assert_eq!(got.fallback, None);
}
