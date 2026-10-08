//! What the native downloader reports as received from the network, against
//! the scripted server of `native_tests.rs`.

use std::sync::{Arc, Mutex};

use super::RawProgress;
use super::native::tests::{Fixture, Reply, TestServer, body_of};
use super::native::{NativeDownload, build_client, download_file};

/// Download `body` from a server answering `reply`, onto `on_disk` bytes of
/// partial file, and return every reading the downloader reported.
async fn readings(reply: Reply, on_disk: &[u8], size: usize) -> Vec<RawProgress> {
    let fx = Fixture::new();
    std::fs::write(fx.part(), on_disk).unwrap();
    let server = TestServer::start(vec![reply]).await;
    let url = server.url();

    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let client = build_client();
    let request = NativeDownload {
        url: &url,
        dest: &fx.dest,
        token: None,
        expected_size: Some(size as u64),
        progress: Some(Arc::new(move |raw| sink.lock().unwrap().push(raw))),
        cancel: None,
    };
    download_file(&client, &request)
        .await
        .expect("download succeeds");

    seen.lock().unwrap().clone()
}

/// A resumed transfer has the bytes of the partial file on disk and has
/// received none of them: only what this attempt fetches came off the network.
#[tokio::test]
async fn a_206_reports_received_from_this_attempt() {
    let body = body_of(4096);
    let reply = Reply::File {
        body: body.clone(),
        etag: None,
    };

    let seen = readings(reply, &body[..1000], body.len()).await;

    let (first, last) = (seen[0], seen[seen.len() - 1]);
    assert_eq!((first.written, first.received), (1000, 0));
    assert_eq!((last.written, last.received), (4096, 3096));
}

/// A server that ignores the range sends the whole file, so the partial file
/// is thrown away and every byte of the file is received in this attempt.
#[tokio::test]
async fn a_200_after_a_range_restarts_received() {
    let body = body_of(4096);
    let reply = Reply::IgnoresRange { body: body.clone() };

    let seen = readings(reply, &[0xff_u8; 1000], body.len()).await;

    let (first, last) = (seen[0], seen[seen.len() - 1]);
    assert_eq!((first.written, first.received), (0, 0));
    assert_eq!((last.written, last.received), (4096, 4096));
}
