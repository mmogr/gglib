//! An image sent to the far machine and one read back: what the far proxy
//! is asked, and that a slow tunnel does not cut the exchange off.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use gglib_core::domain::AttachmentId;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::super::{FarProxy, build, streaming_builder};
use crate::remote::paired_machine::FarCredentials;

/// How long the bounded client may take end to end in these tests, and how
/// long the far proxy waits before it answers: longer.
const BOUND: Duration = Duration::from_millis(100);
const DELAY: Duration = Duration::from_millis(600);

/// One request the far proxy read: its head, and its body.
#[derive(Default)]
struct Seen {
    head: String,
    body: Vec<u8>,
}

/// A far proxy on a loopback port that reads one request whole, waits
/// [`DELAY`], and answers `status` with `body` as `content_type`.
async fn far_proxy(
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
) -> (Arc<Mutex<Seen>>, FarProxy) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Seen::default()));
    let record = Arc::clone(&seen);
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            socket.read_exact(&mut byte).await.unwrap();
            head.push(byte[0]);
        }
        let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .map_or(0, |n| n.trim().parse::<usize>().unwrap());
        let mut sent = vec![0u8; length];
        socket.read_exact(&mut sent).await.unwrap();
        *record.lock().unwrap() = Seen { head, body: sent };
        tokio::time::sleep(DELAY).await;
        let start = format!(
            "HTTP/1.1 {status} X\r\ncontent-type: {content_type}\r\n\
             content-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(start.as_bytes()).await.unwrap();
        socket.write_all(&body).await.unwrap();
    });
    let credentials = FarCredentials {
        key: "sk-the-key".to_owned(),
        fingerprint: "0a1b2c3d4e5f".to_owned(),
        name: None,
    };
    let bounded = build(gglib_proxy::loopback::client_builder().timeout(BOUND)).unwrap();
    let streaming = build(streaming_builder(Duration::from_secs(10))).unwrap();
    let far = FarProxy::with_clients(
        &format!("http://127.0.0.1:{port}/v1"),
        &credentials,
        bounded,
        streaming,
    );
    (seen, far)
}

/// The bytes go to `POST /v1/attachments` as they came, with the key and the
/// caller's content type, and the far machine's answer comes back although
/// it took longer than a bounded request may: the streaming client sent it.
#[tokio::test]
async fn an_upload_is_posted_whole_and_outlasts_the_bounded_limit() {
    let image: Vec<u8> = (0..=255u8).cycle().take(70_000).collect();
    let (seen, far) = far_proxy(200, "application/json", br#"{"id":"x"}"#.to_vec()).await;

    let answer = far
        .upload_attachment(image.clone(), Some("image/png"))
        .await
        .expect("an answer slower than the bounded limit still arrives");

    assert_eq!(answer.status(), 200);
    assert_eq!(answer.text().await.unwrap(), r#"{"id":"x"}"#);
    let seen = seen.lock().unwrap();
    assert!(
        seen.head.starts_with("post /v1/attachments http/1.1"),
        "{}",
        seen.head
    );
    assert!(
        seen.head.contains("authorization: bearer sk-the-key"),
        "{}",
        seen.head
    );
    assert!(
        seen.head.contains("content-type: image/png"),
        "{}",
        seen.head
    );
    assert!(seen.body == image, "the body is the image, byte for byte");
}

/// An image is read at `GET /v1/attachments/{id}` with the key, and its
/// bytes and type come back as the far machine sent them, through the
/// streaming client.
#[tokio::test]
async fn a_fetch_reads_the_image_by_its_id_and_outlasts_the_bounded_limit() {
    let image = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
    let id = AttachmentId::of(&image);
    let (seen, far) = far_proxy(200, "image/png", image.clone()).await;

    let answer = far
        .fetch_attachment(&id)
        .await
        .expect("an answer slower than the bounded limit still arrives");

    let content_type = answer.headers()[reqwest::header::CONTENT_TYPE].clone();
    assert_eq!(content_type, "image/png");
    assert_eq!(answer.bytes().await.unwrap().as_ref(), image.as_slice());
    let seen = seen.lock().unwrap();
    assert!(
        seen.head
            .starts_with(&format!("get /v1/attachments/{id} http/1.1")),
        "{}",
        seen.head
    );
    assert!(
        seen.head.contains("authorization: bearer sk-the-key"),
        "{}",
        seen.head
    );
    assert!(seen.body.is_empty());
}

/// The bounded client, asked the same of the same far proxy, gives up: so
/// the two tests above pass only on the streaming one.
#[tokio::test]
async fn the_bounded_client_would_have_given_up() {
    let (_seen, far) = far_proxy(200, "application/json", b"{}".to_vec()).await;
    assert!(far.list_chats().await.is_err());
}
