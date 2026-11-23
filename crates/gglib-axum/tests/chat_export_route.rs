//! `GET /api/conversations/{id}/export`: a chat as the page's Export saves
//! it, each image's bytes in it, the same as `gglib chat export` writes.
//!
//! That the route asks for the daemon's token is `daemon_token_door`'s
//! sweep, which walks it with every other `/api` route.

mod common;

use axum::body::Body;
use axum::http::{Method, StatusCode};
use gglib_core::CorsConfig;
use gglib_core::domain::chat::{MessageRole, NewConversation, NewMessage};
use serde_json::json;

use common::harness::test_state_and_app;
use common::origin::{HOST, authed, call_json, send_request};

/// A PNG's signature and `IHDR` at `width` by `height`.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    bytes.extend(b"IHDR");
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0]);
    bytes
}

#[tokio::test]
async fn a_chat_exports_with_its_image_as_the_core_exports_it() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let image = state.core.attachments().ingest(&png(4, 2)).await.unwrap();
    let history = state.core.chat_history();
    let id = history
        .create_conversation(NewConversation {
            title: "A screenshot".into(),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    history
        .save_message(NewMessage {
            conversation_id: id,
            role: MessageRole::User,
            content: "What is this?".to_owned(),
            metadata: None,
            images: vec![image.info.id.clone()],
        })
        .await
        .unwrap();

    let request = authed()
        .method(Method::GET)
        .uri(format!("/api/conversations/{id}/export"))
        .header("host", HOST)
        .body(Body::empty())
        .unwrap();
    let answer = send_request(&app, request).await;

    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    let exported = state.core.export_chat(id).await.unwrap();
    assert_eq!(
        answer.body,
        exported.text(),
        "the file's text, as it is saved"
    );
    let file: serde_json::Value = serde_json::from_str(&answer.body).unwrap();
    let bytes = state.core.attachments().blob(&image.info.id).await.unwrap();
    assert_eq!(
        file["messages"][0]["images"][0]["data_url"],
        json!(bytes.data_url())
    );
}

#[tokio::test]
async fn no_chat_by_that_id_is_not_found() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let (status, body) = call_json(&app, Method::GET, "/api/conversations/99/export", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}
