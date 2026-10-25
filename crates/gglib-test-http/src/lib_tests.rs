use std::io::ErrorKind;
use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

use super::{accept, incoming};

/// A listener, and a client that has sent `bytes` and then, if `close`, shut
/// its write side.
async fn sent(bytes: &'static [u8], close: bool) -> (TcpListener, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    client.write_all(bytes).await.unwrap();
    if close {
        client.shutdown().await.unwrap();
    }
    (listener, client)
}

/// A request is read through its `content-length` body, and every part of it
/// is kept as sent: the target's query and escapes, the head with its blank
/// line, a header found whatever its case.
#[tokio::test]
async fn a_request_is_read_through_its_body() {
    let (listener, _client) = sent(
        b"POST /v1/models/a%2Fb?x=1 HTTP/1.1\r\nHost: here\r\nContent-Length: 5\r\n\r\nhello",
        false,
    )
    .await;

    let (request, _) = accept(&listener).await.unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/models/a%2Fb?x=1");
    assert_eq!(request.line(), "POST /v1/models/a%2Fb?x=1 HTTP/1.1");
    assert!(
        request.head.ends_with("Content-Length: 5\r\n\r\n"),
        "{:?}",
        request.head
    );
    assert_eq!(request.header("content-length"), Some("5"));
    assert_eq!(request.header("HOST"), Some("here"));
    assert_eq!(request.body_text(), "hello");
}

/// A request with neither length nor chunks has no body, and does not wait
/// for one.
#[tokio::test]
async fn a_get_has_no_body() {
    let (listener, _client) = sent(b"GET /health HTTP/1.1\r\n\r\n", false).await;

    let (request, _) = accept(&listener).await.unwrap();

    assert_eq!(request.line(), "GET /health HTTP/1.1");
    assert!(request.body.is_empty());
}

/// A chunked body is decoded, its trailers are read, and the reader stops at
/// the blank line that ends it: what the client sends next is still there.
#[tokio::test]
async fn a_chunked_body_is_decoded_up_to_its_end() {
    let (listener, _client) = sent(
        b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n\
          5;name=value\r\nhello\r\n6\r\n world\r\n0\r\nX-Trailer: 1\r\n\r\nnext",
        false,
    )
    .await;

    let (request, mut stream) = accept(&listener).await.unwrap();
    let mut next = [0; 4];
    stream.read_exact(&mut next).await.unwrap();

    assert_eq!(request.body_text(), "hello world");
    assert_eq!(&next, b"next");
}

/// With no trailers, a chunked body ends at the bare `\r\n` after its last
/// chunk, and nothing past it is read.
#[tokio::test]
async fn a_chunked_body_without_trailers_ends_at_its_blank_line() {
    let (listener, _client) = sent(
        b"POST / HTTP/1.1\r\ntransfer-encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\nnext",
        false,
    )
    .await;

    let (request, mut stream) = accept(&listener).await.unwrap();
    let mut next = [0; 4];
    stream.read_exact(&mut next).await.unwrap();

    assert_eq!(request.body_text(), "abc");
    assert_eq!(&next, b"next");
}

/// Nothing past the body is read, so a test can go on reading the stream:
/// to see the client close it, or to read what it sends next.
#[tokio::test]
async fn the_bytes_after_a_request_are_left_on_the_stream() {
    let (listener, _client) =
        sent(b"POST / HTTP/1.1\r\ncontent-length: 2\r\n\r\nokafter", true).await;

    let (request, mut stream) = accept(&listener).await.unwrap();
    let mut rest = String::new();
    stream.read_to_string(&mut rest).await.unwrap();

    assert_eq!(request.body_text(), "ok");
    assert_eq!(rest, "after");
}

/// When a head names both, the body is chunked, as HTTP/1.1 says, and the
/// `content-length` is not read as the body's size.
#[tokio::test]
async fn a_chunked_body_wins_over_a_content_length() {
    let (listener, _client) = sent(
        b"POST / HTTP/1.1\r\ncontent-length: 2\r\ntransfer-encoding: chunked\r\n\r\n\
          3\r\nabc\r\n0\r\n\r\nnext",
        false,
    )
    .await;

    let (request, mut stream) = accept(&listener).await.unwrap();
    let mut next = [0; 4];
    stream.read_exact(&mut next).await.unwrap();

    assert_eq!(request.body_text(), "abc");
    assert_eq!(&next, b"next");
}

/// A chunk with more data than its size line said is refused.
#[tokio::test]
async fn a_chunk_longer_than_its_size_is_invalid() {
    let (listener, _client) = sent(
        b"POST / HTTP/1.1\r\ntransfer-encoding: chunked\r\n\r\n2\r\nabc\r\n0\r\n\r\n",
        true,
    )
    .await;

    let error = accept(&listener).await.unwrap_err();

    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

/// A client that connects and leaves without a byte is an error the fake can
/// shrug off, never a panic.
#[tokio::test]
async fn a_connection_closed_before_a_byte_is_unexpected_eof() {
    let (listener, _client) = sent(b"", true).await;

    let error = accept(&listener).await.unwrap_err();

    assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
}

/// A body cut short by the client closing is an error, not a short body.
#[tokio::test]
async fn a_body_cut_short_is_unexpected_eof() {
    let (listener, _client) = sent(b"POST / HTTP/1.1\r\ncontent-length: 10\r\n\r\nabc", true).await;

    let error = accept(&listener).await.unwrap_err();

    assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
}

/// A length that is not a number is refused rather than read as no body.
#[tokio::test]
async fn a_length_that_is_not_a_number_is_invalid() {
    let (listener, _client) = sent(b"POST / HTTP/1.1\r\ncontent-length: ten\r\n\r\n", false).await;

    let error = accept(&listener).await.unwrap_err();

    assert_eq!(error.kind(), ErrorKind::InvalidData);
}

/// A head that never ends is refused at the cap rather than read forever.
#[tokio::test]
async fn a_head_that_never_ends_is_invalid() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let writer = tokio::spawn(async move {
        let mut head = b"GET / HTTP/1.1\r\n".to_vec();
        head.resize(super::request::MAX_HEAD + 16, b'x');
        // The reader stops at the cap; the rest of the write may fail. The
        // write side then closes, so a reader with no cap ends in EOF rather
        // than waiting.
        let _ = client.write_all(&head).await;
        let _ = client.shutdown().await;
        client
    });

    let error = accept(&listener).await.unwrap_err();

    assert_eq!(error.kind(), ErrorKind::InvalidData);
    drop(writer.await);
}

/// `incoming` returns as soon as the connection is made, before the client
/// has sent anything, and the request is read only when it is asked for.
#[tokio::test]
async fn incoming_does_not_wait_for_the_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();

    let taken = tokio::time::timeout(Duration::from_secs(5), incoming(&listener))
        .await
        .expect("the connection is taken without a request")
        .unwrap();
    client
        .write_all(b"GET /late HTTP/1.1\r\n\r\n")
        .await
        .unwrap();
    let (request, _) = taken.request().await.unwrap();

    assert_eq!(request.path, "/late");
}

/// A connection held unread, then dropped, sends the client nothing.
#[tokio::test]
async fn a_connection_dropped_unread_sends_nothing() {
    let (listener, mut client) = sent(b"GET / HTTP/1.1\r\n\r\n", false).await;

    drop(incoming(&listener).await.unwrap());
    let mut answer = Vec::new();
    let read = client.read_to_end(&mut answer).await;

    assert!(answer.is_empty(), "{answer:?}");
    assert!(
        read.is_ok() || read.is_err_and(|e| e.kind() == ErrorKind::ConnectionReset),
        "the connection ends without an answer"
    );
}
