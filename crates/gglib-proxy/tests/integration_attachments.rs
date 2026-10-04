//! `/v1/attachments` on the real proxy: a paired device sends an image and
//! reads one back, and nothing else reaches them.
//!
//! The hub's chats are the stand-in `fixtures::chats`, whose images go
//! through the real ingest, so what is refused here is refused by the one
//! rule every surface shares.

use std::sync::Arc;

use gglib_core::domain::AttachmentId;
use gglib_core::request_pipeline::{MAX_IMAGE_BYTES, estimate_image_tokens};
use reqwest::{Client, RequestBuilder, StatusCode};

mod fixtures;
use fixtures::chats::{FakeChats, png, serve};
use fixtures::runs::{code, json};
use fixtures::tunnel::DEVICE;

/// An id no image has.
fn unknown() -> AttachmentId {
    AttachmentId::of(b"never uploaded")
}

/// Both routes, as a client reaches them.
fn routes(base: &str) -> Vec<RequestBuilder> {
    let client = Client::new();
    vec![
        client
            .post(format!("{base}/v1/attachments"))
            .body(png(640, 480, 0)),
        client.get(format!("{base}/v1/attachments/{}", unknown())),
    ]
}

/// A request as the tunnel edge marks one from a named device.
fn from_device(request: RequestBuilder) -> RequestBuilder {
    request
        .header("via", "1.1 modelpipe")
        .header("x-modelpipe-device", DEVICE)
}

fn upload(base: &str, bytes: Vec<u8>) -> RequestBuilder {
    from_device(
        Client::new()
            .post(format!("{base}/v1/attachments"))
            .body(bytes),
    )
}

/// An upload answers the image's id, type, size and estimate, and a fetch
/// by that id answers the bytes that were sent, typed and not to be stored.
#[tokio::test]
async fn a_named_device_sends_an_image_and_reads_it_back() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    let image = png(640, 480, 100);

    // The type the client claims is not what decides.
    let request = upload(&base, image.clone()).header("content-type", "text/plain");
    let (status, body) = json(request.send().await.unwrap()).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let id = AttachmentId::of(&image);
    assert_eq!(
        body,
        serde_json::json!({
            "id": id.as_str(),
            "mime": "image/png",
            "width": 640,
            "height": 480,
            "image_tokens": estimate_image_tokens(640, 480),
        })
    );

    let fetch = from_device(Client::new().get(format!("{base}/v1/attachments/{id}")));
    let answer = fetch.send().await.unwrap();
    assert_eq!(answer.status(), StatusCode::OK);
    assert_eq!(answer.headers()["content-type"], "image/png");
    assert_eq!(answer.headers()["cache-control"], "no-store");
    assert_eq!(answer.headers()["x-content-type-options"], "nosniff");
    assert_eq!(answer.bytes().await.unwrap().as_ref(), image.as_slice());
    cancel.cancel();
}

/// An image over the limit is 413 by its code, at the limit it is stored,
/// and neither answer is the extractor's plain text.
#[tokio::test]
async fn an_image_over_the_limit_is_413_image_too_large() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    let header = png(8, 8, 0).len();

    let over = png(8, 8, MAX_IMAGE_BYTES + 1 - header);
    let (status, body) = json(upload(&base, over).send().await.unwrap()).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    assert_eq!(code(&body), "image_too_large", "{body}");
    assert_eq!(body["error"]["type"], "invalid_request_error");
    assert!(body.to_string().contains("8 MiB"), "{body}");
    assert_eq!(chats.images(), 0);

    let at = png(8, 8, MAX_IMAGE_BYTES - header);
    let answer = upload(&base, at).send().await.unwrap();
    assert_eq!(answer.status(), StatusCode::OK);
    assert_eq!(chats.images(), 1);
    cancel.cancel();
}

/// A file that is not a PNG or a JPEG is refused by name, whatever type it
/// was sent as, and nothing is stored.
#[tokio::test]
async fn a_file_that_is_no_image_is_400_unsupported_image() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    let files: [&[u8]; 3] = [b"GIF89a\x01\0\x01\0", b"%PDF-1.7 zzq-private-words", b""];
    for file in files {
        let request = upload(&base, file.to_vec()).header("content-type", "image/png");
        let (status, body) = json(request.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(code(&body), "unsupported_image", "{body}");
        let said = body["error"]["message"].as_str().unwrap();
        assert!(said.contains("PNG") && said.contains("JPEG"), "{said}");
        assert!(!said.contains("zzq-private-words"), "{said}");
    }
    assert_eq!(chats.images(), 0);
    cancel.cancel();
}

/// An id no image has is a 404 with the code a client matches on; a path
/// that is no id is the same, and is not echoed.
#[tokio::test]
async fn an_unknown_id_is_404_attachment_not_found() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    for id in [unknown().to_string(), "zzq-private-words".to_owned()] {
        let request = from_device(Client::new().get(format!("{base}/v1/attachments/{id}")));
        let (status, body) = json(request.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(code(&body), "attachment_not_found", "{body}");
        assert!(!body.to_string().contains("zzq-private-words"), "{body}");
    }
    cancel.cancel();
}

/// This machine sends its images at `/api`; a local client, or one holding
/// the key on a LAN bind, reaches none here.
#[tokio::test]
async fn a_request_not_tunnelled_from_a_named_device_is_refused() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(None, Some(Arc::clone(&chats))).await;
    for request in routes(&base) {
        let (status, body) = json(request.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(code(&body), "device_not_named", "{body}");
    }
    assert_eq!(chats.calls(), 0, "nothing reached the chats");
    cancel.cancel();
}

/// The routes are in the protected group: the key is asked for first, and
/// a tunnelled request that names no device is the gate's to refuse.
#[tokio::test]
async fn the_images_sit_behind_the_bearer_and_the_device_gate() {
    let chats = Arc::new(FakeChats::default());
    let (base, cancel) = serve(Some("secret123"), Some(Arc::clone(&chats))).await;
    for request in routes(&base) {
        let (status, body) = json(from_device(request).send().await.unwrap()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    }
    for request in routes(&base) {
        let request = request
            .bearer_auth("secret123")
            .header("via", "1.1 modelpipe");
        let (status, body) = json(request.send().await.unwrap()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(code(&body), "device_not_paired", "{body}");
    }
    assert_eq!(chats.calls(), 0);
    let sent = upload(&base, png(640, 480, 0)).bearer_auth("secret123");
    assert_eq!(sent.send().await.unwrap().status(), StatusCode::OK);
    cancel.cancel();
}

#[tokio::test]
async fn a_proxy_without_the_chats_answers_503_with_a_code() {
    let (base, cancel) = serve(None, None).await;
    for request in routes(&base) {
        let (status, body) = json(from_device(request).send().await.unwrap()).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        assert_eq!(code(&body), "chats_unavailable");
    }
    cancel.cancel();
}
