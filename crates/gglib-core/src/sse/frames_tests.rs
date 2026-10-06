//! Where lines and events are cut, and what an incomplete event may hold.

use super::*;

/// The frames of a stream that arrives as `chunks`.
fn frames_of(chunks: &[&[u8]]) -> Vec<String> {
    let mut frames = DataFrames::new(1024);
    chunks.iter().flat_map(|chunk| frames.push(chunk)).collect()
}

/// The events of a stream that arrives as `chunks`.
fn events_of(chunks: &[&[u8]]) -> Vec<Event> {
    let mut frames = DataFrames::new(1024);
    chunks
        .iter()
        .flat_map(|chunk| frames.push_events(chunk))
        .collect()
}

fn event(id: Option<&str>, name: Option<&str>, data: &str) -> Event {
    Event {
        id: id.map(str::to_owned),
        name: name.map(str::to_owned),
        data: data.to_owned(),
    }
}

#[test]
fn a_line_is_returned_only_once_its_newline_arrives() {
    let mut lines = Lines::new();
    lines.push(b"one\ntw");
    assert_eq!(lines.next_line().as_deref(), Some(&b"one"[..]));
    assert_eq!(lines.next_line(), None);
    assert_eq!(lines.held(), 2, "the start of the next line is kept");
    lines.push(b"o\n");
    assert_eq!(lines.next_line().as_deref(), Some(&b"two"[..]));
    assert_eq!(lines.held(), 0);
}

#[test]
fn a_chunk_that_ends_between_cr_and_lf_still_ends_one_line() {
    let mut lines = Lines::new();
    lines.push(b"one\r");
    assert_eq!(lines.next_line(), None, "a lone \\r ends nothing");
    lines.push(b"\n\r\n");
    assert_eq!(lines.next_line().as_deref(), Some(&b"one"[..]));
    assert_eq!(lines.next_line().as_deref(), Some(&b""[..]));
    assert_eq!(lines.next_line(), None);
}

#[test]
fn every_cr_before_the_lf_is_dropped_and_one_inside_the_line_is_kept() {
    let mut lines = Lines::new();
    lines.push(b"a\rb\r\r\n");
    assert_eq!(lines.next_line().as_deref(), Some(&b"a\rb"[..]));
}

#[test]
fn an_event_is_taken_only_once_its_blank_line_arrives() {
    let mut frames = DataFrames::new(1024);
    assert!(frames.push(b"data: {\"a\":").is_empty());
    assert_eq!(frames.push(b"1}\n\ndata: [DONE]"), ["{\"a\":1}"]);
    assert_eq!(frames.push(b"\n\n"), ["[DONE]"]);
}

#[test]
fn comments_crlf_and_multi_line_data_are_handled() {
    let mut frames = DataFrames::new(1024);
    let got = frames.push(b": keep-alive\n\nid: 3\r\ndata: one\r\ndata:two\r\n\r\n");
    assert_eq!(got, ["one\ntwo"]);
}

#[test]
fn a_stream_cut_at_every_byte_yields_the_frames_it_yields_whole() {
    let stream = ": keep-alive\r\n\r\nid: 3\r\ndata: caf\u{e9} \u{1f680}\r\ndata:\u{4f60}\u{597d}\r\n\r\ndata: [DONE]\n\n";
    let bytes = stream.as_bytes();
    let whole = frames_of(&[bytes]);
    assert_eq!(whole, ["caf\u{e9} \u{1f680}\n\u{4f60}\u{597d}", "[DONE]"]);
    for cut in 1..bytes.len() {
        assert_eq!(
            frames_of(&[&bytes[..cut], &bytes[cut..]]),
            whole,
            "cut at byte {cut}"
        );
    }
}

#[test]
fn a_blank_line_ends_an_event_whichever_ending_each_line_has() {
    assert_eq!(
        frames_of(&[b"data: one\r\n\ndata: two\n\r\n"]),
        ["one", "two"]
    );
}

#[test]
fn an_incomplete_event_past_the_limit_is_an_overflow() {
    let mut frames = DataFrames::new(16);
    assert_eq!(frames.push(b"data: 1\n\ndata: 0123456789"), ["1"]);
    assert!(
        !frames.overflowed(),
        "16 bytes held is the limit, not past it"
    );
    assert!(frames.push(b"7").is_empty());
    assert!(frames.overflowed());
}

#[test]
fn the_limit_counts_every_line_of_the_incomplete_event() {
    let mut frames = DataFrames::new(16);
    assert!(frames.push(b": note\r\ndata: 1\n").is_empty());
    assert!(
        !frames.overflowed(),
        "two whole lines of 8 bytes each are the limit, not past it"
    );
    assert!(frames.push(b"d").is_empty());
    assert!(
        frames.overflowed(),
        "the lines already read still count towards the event"
    );
}

#[test]
fn a_character_split_across_reads_is_decoded_whole() {
    let mut frames = DataFrames::new(1024);
    let bytes = "data: caf\u{e9}\n\n".as_bytes();
    assert!(frames.push(&bytes[..10]).is_empty());
    assert_eq!(frames.push(&bytes[10..]), ["caf\u{e9}"]);
}

#[test]
fn bytes_that_are_not_utf8_are_replaced_in_a_frame_not_refused() {
    assert_eq!(frames_of(&[b"data: a\xffb\n\n"]), ["a\u{fffd}b"]);
}

#[test]
fn an_event_carries_its_own_id_and_name_and_none_from_the_one_before() {
    assert_eq!(
        events_of(&[b"id: 7\nevent: run\ndata: one\n\ndata: two\n\n"]),
        [
            event(Some("7"), Some("run"), "one"),
            event(None, None, "two")
        ]
    );
}

#[test]
fn an_event_without_data_is_dropped_and_its_fields_with_it() {
    assert_eq!(
        events_of(&[b"id: 7\nevent: run\n\ndata: one\n\n"]),
        [event(None, None, "one")]
    );
}

#[test]
fn a_field_loses_the_one_space_after_its_colon_and_no_more() {
    assert_eq!(
        events_of(&[b"id:7\nevent:  run\ndata:  one \n\n"]),
        [event(Some("7"), Some(" run"), " one ")]
    );
}

#[test]
fn a_line_that_only_starts_like_a_field_is_not_that_field() {
    assert_eq!(
        events_of(&[b"identity: 7\nevents: run\ndatum: no\ndata: one\n\n"]),
        [event(None, None, "one")]
    );
}

#[test]
fn events_cut_at_every_byte_are_the_events_read_whole() {
    let stream = "id: 12\r\nevent: run\r\ndata: caf\u{e9}\r\n\r\nid: 13\ndata: \u{1f680}\n\n";
    let bytes = stream.as_bytes();
    let whole = events_of(&[bytes]);
    assert_eq!(
        whole,
        [
            event(Some("12"), Some("run"), "caf\u{e9}"),
            event(Some("13"), None, "\u{1f680}")
        ]
    );
    for cut in 1..bytes.len() {
        assert_eq!(
            events_of(&[&bytes[..cut], &bytes[cut..]]),
            whole,
            "cut at byte {cut}"
        );
    }
}

#[test]
fn a_splitter_with_no_limit_never_overflows() {
    let mut frames = DataFrames::unbounded();
    assert!(frames.push(&[b'x'; 4096]).is_empty());
    assert!(!frames.overflowed());
}
