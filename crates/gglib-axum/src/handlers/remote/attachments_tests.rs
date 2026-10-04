//! An image sent to the far machine and one read back, against a fake far
//! proxy on a loopback port: the path each reaches, the key and the body it
//! carries, and what comes back.

use axum::body::Bytes;
use axum::http::{StatusCode, header};
use gglib_core::domain::AttachmentId;

use super::super::fake_far::{carries_key, far, json, only, read};
use super::{fetch_via, upload_via};
use crate::error::HttpError;

#[tokio::test]
async fn an_upload_reaches_the_far_store_with_the_key_and_its_answer_comes_back() {
    let stored = r#"{"id":"x","mime":"image/png","width":640,"height":480,"image_tokens":300}"#;
    let (fake, far) = far(200, stored).await;
    let image = Bytes::from_static(b"\x89PNG the bytes as they came");

    let answer = upload_via(&far, image.clone(), Some("image/png")).await;
    let (status, body) = read(answer.unwrap()).await;

    assert_eq!((status, body.as_str()), (StatusCode::OK, stored));
    let seen = only(&fake);
    assert_eq!(
        (seen.method.as_str(), seen.uri.as_str()),
        ("POST", "/v1/attachments")
    );
    assert!(carries_key(&seen), "{seen:?}");
    assert_eq!(
        seen.body.as_bytes(),
        String::from_utf8_lossy(&image).as_bytes()
    );
}

/// The far machine's refusal keeps its status and its code, in this
/// daemon's error shape.
#[tokio::test]
async fn a_far_refusal_of_an_upload_keeps_its_status_and_code() {
    let refusal = r#"{"error":{"message":"The image is larger than 8 MiB, the most one image may be.","type":"invalid_request_error","code":"image_too_large"}}"#;
    let (_, far) = far(413, refusal).await;

    let answer = upload_via(&far, Bytes::from_static(b"x"), None).await;
    let (status, body) = read(answer.unwrap()).await;

    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        json(&body),
        serde_json::json!({
            "error": "The image is larger than 8 MiB, the most one image may be.",
            "status": 413,
            "type": "image_too_large",
        })
    );
}

#[tokio::test]
async fn a_fetch_reaches_the_image_by_its_id_and_is_not_kept() {
    let (fake, far) = far(200, "the image").await;
    *fake.content_type.lock().unwrap() = "image/jpeg";
    let id = AttachmentId::of(b"an image");

    let response = fetch_via(&far, id.as_str()).await.unwrap();

    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        response.headers()[header::X_CONTENT_TYPE_OPTIONS],
        "nosniff"
    );
    let (status, body) = read(response).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "the image"));
    let seen = only(&fake);
    assert_eq!(
        (seen.method.as_str(), seen.uri.as_str()),
        ("GET", format!("/v1/attachments/{id}").as_str())
    );
    assert!(carries_key(&seen), "{seen:?}");
}

/// This daemon serves what the far machine answers from its own origin, so
/// a type the store does not keep is not passed on: the bytes come back as
/// bytes of no type, which a browser may neither run nor guess at.
#[tokio::test]
async fn a_fetch_the_far_machine_types_as_no_stored_image_comes_back_untyped() {
    for sent in ["text/html", "image/svg+xml", "image/png; charset=utf-8"] {
        let (fake, far) = far(200, "<script>alert(1)</script>").await;
        *fake.content_type.lock().unwrap() = sent;
        let id = AttachmentId::of(b"an image");

        let response = fetch_via(&far, id.as_str()).await.unwrap();

        let headers = response.headers();
        assert_eq!(
            headers[header::CONTENT_TYPE],
            "application/octet-stream",
            "{sent}"
        );
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    }
    let (fake, far) = far(200, "the image").await;
    *fake.content_type.lock().unwrap() = "image/png";
    let id = AttachmentId::of(b"an image");
    let response = fetch_via(&far, id.as_str()).await.unwrap();
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
}

/// An id the far machine does not hold is its 404 and its code, and is not
/// kept.
#[tokio::test]
async fn a_far_404_for_an_unknown_id_comes_back_with_its_code() {
    let refusal = r#"{"error":{"message":"No stored image has that id.","type":"invalid_request_error","code":"attachment_not_found"}}"#;
    let (_, far) = far(404, refusal).await;
    let id = AttachmentId::of(b"never uploaded");

    let response = fetch_via(&far, id.as_str()).await.unwrap();

    assert!(response.headers().get(header::CACHE_CONTROL).is_none());
    let (status, body) = read(response).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json(&body)["type"], "attachment_not_found");
}

/// A path that is not an id is refused here: nothing but an id is ever put
/// in the far machine's path, and nothing is sent.
#[tokio::test]
async fn a_path_that_is_no_id_is_refused_before_anything_is_sent() {
    let (fake, far) = far(200, "the image").await;

    for path in ["..", "x/../chats", "shot.png", ""] {
        let Err(HttpError::Coded { status, code, .. }) = fetch_via(&far, path).await else {
            panic!("{path:?} was sent");
        };
        assert_eq!(
            (status, code),
            (StatusCode::NOT_FOUND, "attachment_not_found")
        );
    }
    assert!(fake.seen.lock().unwrap().is_empty());
}
