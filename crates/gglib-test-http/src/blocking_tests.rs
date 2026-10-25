//! The std reader shares its byte rules with the tokio one (`request.rs`) but
//! has a read loop of its own, so these run every path through that loop: a
//! body by length, several chunks with an extension and trailers, an
//! over-long chunk, a head `request.rs` refuses, a head over the cap, and
//! early ends.

use std::io::{ErrorKind, Read as _, Write as _};
use std::net::{Shutdown, TcpListener, TcpStream};

use super::{accept, incoming};

fn sent(bytes: &[u8], close: bool) -> (TcpListener, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client.write_all(bytes).unwrap();
    if close {
        client.shutdown(Shutdown::Write).unwrap();
    }
    (listener, client)
}

#[test]
fn a_request_is_read_through_its_body_and_no_further() {
    let (listener, _client) = sent(
        b"POST /a?b=%2F HTTP/1.1\r\nContent-Type: application/json\r\ncontent-length: 2\r\n\r\n{}after",
        true,
    );

    let (request, mut stream) = accept(&listener).unwrap();
    let mut rest = String::new();
    stream.read_to_string(&mut rest).unwrap();

    assert_eq!(request.line(), "POST /a?b=%2F HTTP/1.1");
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.body_text(), "{}");
    assert_eq!(rest, "after");
}

#[test]
fn a_chunked_body_is_decoded_up_to_its_end() {
    let (listener, _client) = sent(
        b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nhi\r\n0\r\n\r\nnext",
        true,
    );

    let (request, mut stream) = accept(&listener).unwrap();
    let mut rest = String::new();
    stream.read_to_string(&mut rest).unwrap();

    assert_eq!(request.body_text(), "hi");
    assert_eq!(rest, "next");
}

#[test]
fn several_chunks_with_an_extension_are_one_body() {
    let (listener, _client) = sent(
        b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n\
          5;name=value\r\nhello\r\n6\r\n world\r\n0\r\n\r\nnext",
        true,
    );

    let (request, mut stream) = accept(&listener).unwrap();
    let mut rest = String::new();
    stream.read_to_string(&mut rest).unwrap();

    assert_eq!(request.body_text(), "hello world");
    assert_eq!(rest, "next");
}

#[test]
fn a_head_the_shared_rules_refuse_is_invalid() {
    let (listener, _client) = sent(b"POST / HTTP/1.1\r\ncontent-length: ten\r\n\r\n", true);

    let error = accept(&listener).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

#[test]
fn a_head_that_never_ends_is_invalid() {
    let mut head = b"GET / HTTP/1.1\r\n".to_vec();
    head.resize(crate::request::MAX_HEAD + 16, b'x');
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    // The reader stops at the cap, so the write runs beside it and may fail;
    // the write side then closes, so a reader with no cap ends in EOF.
    let writer = std::thread::spawn(move || {
        let _ = client.write_all(&head);
        let _ = client.shutdown(Shutdown::Write);
        client
    });

    let error = accept(&listener).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::InvalidData);
    drop(writer.join());
}

#[test]
fn a_chunked_body_with_trailers_ends_at_its_blank_line() {
    let (listener, _client) = sent(
        b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n\
          3\r\nabc\r\n0\r\nX-Trailer: 1\r\n\r\nnext",
        true,
    );

    let (request, mut stream) = accept(&listener).unwrap();
    let mut rest = String::new();
    stream.read_to_string(&mut rest).unwrap();

    assert_eq!(request.body_text(), "abc");
    assert_eq!(rest, "next");
}

#[test]
fn a_chunked_body_wins_over_a_content_length() {
    let (listener, _client) = sent(
        b"POST / HTTP/1.1\r\ncontent-length: 2\r\ntransfer-encoding: chunked\r\n\r\n\
          3\r\nabc\r\n0\r\n\r\nnext",
        true,
    );

    let (request, mut stream) = accept(&listener).unwrap();
    let mut rest = String::new();
    stream.read_to_string(&mut rest).unwrap();

    assert_eq!(request.body_text(), "abc");
    assert_eq!(rest, "next");
}

#[test]
fn a_chunk_longer_than_its_size_is_invalid() {
    let (listener, _client) = sent(
        b"POST / HTTP/1.1\r\ntransfer-encoding: chunked\r\n\r\n2\r\nabc\r\n0\r\n\r\n",
        true,
    );

    let error = accept(&listener).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

#[test]
fn a_connection_closed_before_a_byte_is_unexpected_eof() {
    let (listener, _client) = sent(b"", true);

    let error = accept(&listener).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
}

#[test]
fn a_body_cut_short_is_unexpected_eof() {
    let (listener, _client) = sent(b"POST / HTTP/1.1\r\ncontent-length: 9\r\n\r\nab", true);

    let error = accept(&listener).unwrap_err();

    assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
}

#[test]
fn incoming_takes_the_connection_and_the_request_is_read_later() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();

    let taken = incoming(&listener).unwrap();
    client.write_all(b"GET /late HTTP/1.1\r\n\r\n").unwrap();
    let (request, _) = taken.request().unwrap();

    assert_eq!(request.path, "/late");
}
