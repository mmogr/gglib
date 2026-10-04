//! `/api/attachments` on the router a daemon builds: the chat page sends an
//! image once and reads it back by its id, a file that is no image or is
//! too large is refused by code, and no image reaches a log line.
//!
//! That the routes ask for the daemon's token is `daemon_token_door`'s
//! sweep, which walks them with every other `/api` route.

mod common;

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Method, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::test_app;
use common::origin::{HOST, authed};
use gglib_core::CorsConfig;
use gglib_core::contracts::http::attachments::{
    ATTACHMENTS_PATH, REMOTE_ATTACHMENTS_PATH, attachment_path,
};
use gglib_core::domain::AttachmentId;
use gglib_core::request_pipeline::{MAX_IMAGE_BYTES, estimate_image_tokens};

/// A PNG's signature and `IHDR` for `width` by `height`, then `tail`: a
/// file the ingest reads the size of.
fn png(width: u32, height: u32, tail: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes.extend(tail);
    bytes
}

/// What came back: the status, the headers and the body, read whole.
struct Answer {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Vec<u8>,
}

impl Answer {
    fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).expect("a JSON body")
    }
}

async fn send(app: &Router, method: Method, path: &str, body: Vec<u8>) -> Answer {
    let request = authed()
        .method(method)
        .uri(path)
        .header("host", HOST)
        .header("content-type", "application/octet-stream")
        .body(Body::from(body))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    Answer {
        status,
        headers,
        body: body.to_vec(),
    }
}

async fn upload(app: &Router, image: Vec<u8>) -> Answer {
    send(app, Method::POST, ATTACHMENTS_PATH, image).await
}

/// An upload answers the image's id, type, size and estimate; a fetch by
/// that id answers the bytes that were sent, typed and kept for good; and
/// the same bytes sent again are the same image.
#[tokio::test]
async fn an_image_is_sent_once_and_read_back_by_its_id() {
    let app = test_app(CorsConfig::AllowAll).await;
    let image = png(1280, 720, b"the rest of the file");
    let id = AttachmentId::of(&image);

    let stored = upload(&app, image.clone()).await;

    assert_eq!(stored.status, StatusCode::OK);
    let want = serde_json::json!({
        "id": id.as_str(),
        "mime": "image/png",
        "width": 1280,
        "height": 720,
        "image_tokens": estimate_image_tokens(1280, 720),
    });
    assert_eq!(stored.json(), want);
    assert_eq!(upload(&app, image.clone()).await.json(), want);

    let read = send(&app, Method::GET, &attachment_path(id.as_str()), Vec::new()).await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.headers[header::CONTENT_TYPE], "image/png");
    assert_eq!(
        read.headers[header::CACHE_CONTROL],
        "private, max-age=31536000, immutable"
    );
    assert_eq!(read.headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert!(read.body == image, "the bytes are the ones that were sent");
}

/// An image over the limit is the coded 413, not the extractor's plain
/// text; one at the limit is stored. So the route takes more than axum's
/// 2 MiB default and no more than one image.
#[tokio::test]
async fn an_image_over_the_limit_is_413_image_too_large() {
    let app = test_app(CorsConfig::AllowAll).await;
    let header = png(8, 8, &[]).len();

    let over = png(8, 8, &vec![0; MAX_IMAGE_BYTES + 1 - header]);
    let refused = upload(&app, over).await;
    assert_eq!(refused.status, StatusCode::PAYLOAD_TOO_LARGE);
    let body = refused.json();
    assert_eq!(body["type"], "image_too_large", "{body}");
    assert!(body["error"].as_str().unwrap().contains("8 MiB"), "{body}");

    let at = png(8, 8, &vec![0; MAX_IMAGE_BYTES - header]);
    assert_eq!(upload(&app, at).await.status, StatusCode::OK);
}

/// A file that is not a PNG or a JPEG is refused by name, and is not there
/// to read afterwards.
#[tokio::test]
async fn a_file_that_is_no_image_is_400_unsupported_image() {
    let app = test_app(CorsConfig::AllowAll).await;
    let files: [&[u8]; 3] = [b"GIF89a\x01\0\x01\0", b"%PDF-1.7", b""];
    for file in files {
        let refused = upload(&app, file.to_vec()).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST);
        let body = refused.json();
        assert_eq!(body["type"], "unsupported_image", "{body}");
        let said = body["error"].as_str().unwrap();
        assert!(said.contains("PNG") && said.contains("JPEG"), "{said}");

        let path = attachment_path(AttachmentId::of(file).as_str());
        let read = send(&app, Method::GET, &path, Vec::new()).await;
        assert_eq!(read.status, StatusCode::NOT_FOUND);
    }
}

/// An id no image has is a 404 with the code a client matches on, and so
/// is a path that is no id, which is not echoed.
#[tokio::test]
async fn an_unknown_id_is_404_attachment_not_found() {
    let app = test_app(CorsConfig::AllowAll).await;
    let never = AttachmentId::of(b"never uploaded");
    for id in [never.as_str(), "zzq-private-words"] {
        let read = send(&app, Method::GET, &attachment_path(id), Vec::new()).await;
        assert_eq!(read.status, StatusCode::NOT_FOUND);
        let body = read.json();
        assert_eq!(body["type"], "attachment_not_found", "{body}");
        assert_eq!(body["status"], 404);
        assert!(!body.to_string().contains("zzq-private-words"), "{body}");
    }
}

/// The far machine's store takes one image too: one over the limit is
/// refused here by the same code and is not sent, and one at the limit gets
/// as far as this machine being joined to nothing.
#[tokio::test]
async fn the_far_route_takes_one_image_and_no_more() {
    let app = test_app(CorsConfig::AllowAll).await;
    let header = png(8, 8, &[]).len();

    let at = png(8, 8, &vec![0; MAX_IMAGE_BYTES - header]);
    let fits = send(&app, Method::POST, REMOTE_ATTACHMENTS_PATH, at).await;
    assert_eq!(fits.status, StatusCode::CONFLICT);

    let over = png(8, 8, &vec![0; MAX_IMAGE_BYTES + 1 - header]);
    let refused = send(&app, Method::POST, REMOTE_ATTACHMENTS_PATH, over).await;
    assert_eq!(refused.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(refused.json()["type"], "image_too_large");
}

/// What the privacy test's image carries, to look for afterwards.
const SECRET: &[u8] = b"IMAGE-SECRET-IMAGE-SECRET-IMAGE-SECRET";

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// No byte of an image, raw or as base64, reaches a log line or a refusal:
/// not when it is stored, read, refused as no image, or refused as too
/// large.
#[tokio::test]
async fn no_image_reaches_a_log_line_or_a_refusal() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // As in the runs' own privacy test: a second registered dispatcher makes
    // callsites other threads hit first consult this one too.
    let capture = tracing::Dispatch::new(subscriber);
    let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    let _default = tracing::dispatcher::set_default(&capture);
    tracing::callsite::rebuild_interest_cache();
    tracing::info!("the capture works");

    let app = test_app(CorsConfig::AllowAll).await;
    let image = png(640, 480, SECRET);
    let id = AttachmentId::of(&image);
    // The image as a model is sent it: its header's base64, and its tail's.
    let sent_as = gglib_core::domain::AttachmentBlob {
        mime: "image/png".to_owned(),
        data: image.clone(),
    }
    .data_url();
    let encoded = sent_as.split_once(',').unwrap().1;
    let (head, tail) = (
        &encoded[..40],
        &encoded[encoded.len() - 44..encoded.len() - 4],
    );
    let mut no_image = b"not an image: ".to_vec();
    no_image.extend(SECRET);
    let mut too_large = png(8, 8, SECRET);
    too_large.resize(MAX_IMAGE_BYTES + 1, 0);

    let mut said = Vec::new();
    said.push(upload(&app, image).await.body);
    let read = send(&app, Method::GET, &attachment_path(id.as_str()), Vec::new()).await;
    assert_eq!(read.status, StatusCode::OK);
    said.push(upload(&app, no_image).await.body);
    said.push(upload(&app, too_large).await.body);

    let log = String::from_utf8_lossy(&captured.0.lock().unwrap()).into_owned();
    assert!(log.contains("the capture works"), "{log}");
    let said = said
        .iter()
        .map(|body| String::from_utf8_lossy(body))
        .collect::<String>();
    for (place, text) in [("a log line", &log), ("an answer", &said)] {
        assert!(!text.contains("IMAGE-SECRET"), "the image reached {place}");
        assert!(!text.contains(head), "its base64 reached {place}");
        assert!(!text.contains(tail), "its base64 reached {place}");
    }
}
