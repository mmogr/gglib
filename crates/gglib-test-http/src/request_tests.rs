//! The byte rules both readers share, checked without a socket.

use std::io::ErrorKind;

use super::{
    Body, MAX_BODY, MAX_HEAD, chunk_size, parse_head, push_head_byte, push_line_byte, room_for,
};

fn parsed(head: &str) -> std::io::Result<Body> {
    parse_head(head.as_bytes()).map(|(_, body)| body)
}

fn refused(head: &str) -> ErrorKind {
    parsed(head).unwrap_err().kind()
}

#[test]
fn a_chunked_body_wins_over_a_content_length() {
    let body = parsed("POST / HTTP/1.1\r\ncontent-length: 3\r\ntransfer-encoding: chunked\r\n\r\n");

    assert_eq!(body.unwrap(), Body::Chunked);
}

/// Any transfer-encoding line naming chunked, in any case, frames the body.
#[test]
fn chunked_is_found_in_any_transfer_encoding_line_and_any_case() {
    let body =
        parsed("POST / HTTP/1.1\r\nTransfer-Encoding: gzip\r\nTransfer-Encoding: Chunked\r\n\r\n");

    assert_eq!(body.unwrap(), Body::Chunked);
}

/// A coding that only contains the word is not chunked.
#[test]
fn chunked_is_a_whole_coding_not_part_of_a_word() {
    let body =
        parsed("POST / HTTP/1.1\r\ntransfer-encoding: notchunked\r\ncontent-length: 1\r\n\r\n");

    assert_eq!(body.unwrap(), Body::Length(1));
}

#[test]
fn a_header_name_with_space_before_its_colon_is_refused() {
    assert_eq!(
        refused("POST / HTTP/1.1\r\nContent-Length : 5\r\n\r\n"),
        ErrorKind::InvalidData
    );
    assert_eq!(
        refused("POST / HTTP/1.1\r\n: 5\r\n\r\n"),
        ErrorKind::InvalidData
    );
}

#[test]
fn a_length_is_digits_only() {
    assert_eq!(
        refused("POST / HTTP/1.1\r\ncontent-length: +1\r\n\r\n"),
        ErrorKind::InvalidData
    );
    assert_eq!(
        refused("POST / HTTP/1.1\r\ncontent-length: -1\r\n\r\n"),
        ErrorKind::InvalidData
    );
    assert_eq!(
        refused("POST / HTTP/1.1\r\ncontent-length:\r\n\r\n"),
        ErrorKind::InvalidData
    );
}

#[test]
fn two_lengths_that_disagree_are_refused_and_two_that_agree_are_one() {
    assert_eq!(
        refused("POST / HTTP/1.1\r\ncontent-length: 1\r\ncontent-length: 2\r\n\r\n"),
        ErrorKind::InvalidData
    );
    let body = parsed("POST / HTTP/1.1\r\ncontent-length: 2\r\nContent-Length: 2\r\n\r\n");
    assert_eq!(body.unwrap(), Body::Length(2));
}

#[test]
fn a_length_over_the_cap_is_refused() {
    let head = format!(
        "POST / HTTP/1.1\r\ncontent-length: {}\r\n\r\n",
        MAX_BODY + 1
    );

    assert_eq!(refused(&head), ErrorKind::InvalidData);
}

#[test]
fn a_request_line_without_a_target_is_refused() {
    assert_eq!(refused("GET\r\n\r\n"), ErrorKind::InvalidData);
    assert_eq!(refused("GET  HTTP/1.1\r\n\r\n"), ErrorKind::InvalidData);
    assert_eq!(refused(" / HTTP/1.1\r\n\r\n"), ErrorKind::InvalidData);
}

#[test]
fn a_header_line_without_a_colon_is_refused() {
    assert_eq!(
        refused("GET / HTTP/1.1\r\nno colon here\r\n\r\n"),
        ErrorKind::InvalidData
    );
}

/// A head that is not UTF-8 is read as text with the bad bytes replaced; it
/// is not a panic.
#[test]
fn a_head_that_is_not_utf8_is_read_lossily() {
    let (request, _) = parse_head(b"GET /\xff HTTP/1.1\r\n\r\n").unwrap();

    assert_eq!(request.path, "/\u{fffd}");
}

/// The first of two same-named headers is the one `header` answers.
#[test]
fn header_answers_the_first_of_two() {
    let (request, _) = parse_head(b"GET / HTTP/1.1\r\nX-Seen: one\r\nx-seen: two\r\n\r\n").unwrap();

    assert_eq!(request.header("x-seen"), Some("one"));
}

#[test]
fn a_chunk_size_that_is_not_hex_or_overflows_is_refused() {
    assert_eq!(
        chunk_size(b"zz").unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        chunk_size(b"-1").unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        chunk_size(b"fffffffffffffffffffff").unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        chunk_size(b"+1a").unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(chunk_size(b"").unwrap_err().kind(), ErrorKind::InvalidData);
    assert_eq!(chunk_size(b"1a;name=value").unwrap(), 0x1a);
}

/// The cap is on the whole body, however its chunks add up.
#[test]
fn the_body_cap_is_on_the_total() {
    assert_eq!(room_for(MAX_BODY - 1, 1).unwrap(), MAX_BODY);
    assert_eq!(
        room_for(MAX_BODY, 1).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        room_for(usize::MAX, 1).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn a_head_or_a_chunk_line_that_never_ends_is_refused_at_the_cap() {
    let mut head = Vec::new();
    let mut line = Vec::new();
    for _ in 0..MAX_HEAD - 1 {
        assert!(!push_head_byte(&mut head, b'x').unwrap());
        assert!(!push_line_byte(&mut line, b'x').unwrap());
    }

    assert_eq!(
        push_head_byte(&mut head, b'x').unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(
        push_line_byte(&mut line, b'x').unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}
