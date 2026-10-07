//! `PUT /api/runs/{id}?kind=agent` whose messages carry images: an image
//! not stored, or images over 16 MiB together, history included, is
//! refused by its code before a slot is taken or a model is reached, on
//! this machine's model and the paired machine's alike (#1257).
//!
//! The harness runs no llama-server and no tunnel, so a run whose images
//! pass is refused where any run is: the port, or the missing connection.
//! That later refusal is each test's proof that the check let it through.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, StatusCode};
use gglib_core::CorsConfig;
use gglib_core::contracts::http::daemon::{RUNS_PATH, run_path};
use gglib_core::domain::AttachmentId;
use gglib_core::domain::chat::NewConversation;
use gglib_core::domain::runs::RunList;
use gglib_core::request_pipeline::{MAX_IMAGE_BYTES, MAX_REQUEST_IMAGE_BYTES};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use common::harness::test_state_and_app;
use common::origin::authed;

async fn call(app: &Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut request = authed()
        .method(method)
        .uri(uri)
        .header("Host", "127.0.0.1:9887");
    if body.is_some() {
        request = request.header("content-type", "application/json");
    }
    let body = body.map_or_else(Body::empty, |value| Body::from(value.to_string()));
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

/// A PNG's signature and `IHDR`, then `fill` up to `len` bytes.
fn png(fill: u8, len: usize) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend([0, 0, 0, 13]);
    bytes.extend(b"IHDR");
    bytes.extend(640_u32.to_be_bytes());
    bytes.extend(480_u32.to_be_bytes());
    bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes.resize(len.max(bytes.len()), fill);
    bytes
}

async fn upload(state: &gglib_axum::AppState, bytes: &[u8]) -> String {
    let stored = state.core.attachments().ingest(bytes).await.unwrap();
    stored.info.id.to_string()
}

/// A question with `earlier`, its answer, and a question with `last`.
fn messages(earlier: &[&str], last: &[&str]) -> Value {
    json!([
        { "role": "user", "content": "this one", "images": earlier },
        { "role": "assistant", "content": "seen" },
        { "role": "user", "content": "and this?", "images": last },
    ])
}

fn local(messages: &Value) -> Value {
    json!({ "port": 9000, "messages": messages })
}

fn far(messages: &Value) -> Value {
    json!({
        "port": 0,
        "messages": messages,
        "far": { "machine": { "kind": "paired", "fingerprint": "0a1b2c3d4e5f" }, "id": 3 },
    })
}

async fn put(app: &Router, body: Value) -> (StatusCode, String) {
    let uri = format!("{}?kind=agent", run_path("a1"));
    let (status, error) = call(app, Method::PUT, &uri, Some(body)).await;
    (
        status,
        error["type"].as_str().unwrap_or_default().to_owned(),
    )
}

async fn no_runs(app: &Router) {
    let (_, list) = call(app, Method::GET, RUNS_PATH, None).await;
    let list: RunList = serde_json::from_value(list).unwrap();
    assert!(list.runs.is_empty(), "{list:?}");
}

fn refusal(code: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, code.to_owned())
}

/// An id the history names that is not stored is refused by its code, on
/// either machine's model; the same history with the image stored goes on
/// to the port, or to the tunnel.
#[tokio::test]
async fn an_image_the_history_names_that_is_not_stored_is_refused_by_code() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let kept = upload(&state, &png(1, 64)).await;
    let gone = AttachmentId::of(b"never uploaded").to_string();

    let unknown = messages(&[&gone], &[]);
    assert_eq!(
        put(&app, local(&unknown)).await,
        refusal("attachment_not_found")
    );
    assert_eq!(
        put(&app, far(&unknown)).await,
        refusal("attachment_not_found")
    );
    no_runs(&app).await;

    let stored = messages(&[&kept], &[]);
    assert_eq!(put(&app, local(&stored)).await, refusal("invalid_request"));
    assert_eq!(put(&app, far(&stored)).await.0, StatusCode::CONFLICT);
}

/// Two images at 8 MiB each are the 16 MiB a request may carry; the
/// history's image and the turn's add up, and one image more is refused.
#[tokio::test]
async fn images_over_16_mib_together_are_refused_by_code() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    assert_eq!(MAX_REQUEST_IMAGE_BYTES, 2 * MAX_IMAGE_BYTES);
    let big = upload(&state, &png(2, MAX_IMAGE_BYTES)).await;
    let small = upload(&state, &png(3, 64)).await;
    let conversation = state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            ..NewConversation::default()
        })
        .await
        .unwrap();

    let mut over = local(&messages(&[&big], &[&big, &small]));
    over["conversation_id"] = json!(conversation);
    assert_eq!(put(&app, over).await, refusal("request_images_too_large"));
    assert_eq!(
        put(&app, far(&messages(&[&big], &[&big, &small]))).await,
        refusal("request_images_too_large")
    );
    no_runs(&app).await;
    let saved = state.core.chat_history().get_messages(conversation).await;
    assert!(saved.unwrap().is_empty());

    let at = messages(&[&big], &[&big]);
    assert_eq!(put(&app, local(&at)).await, refusal("invalid_request"));
    assert_eq!(put(&app, far(&at)).await.0, StatusCode::CONFLICT);
}

/// The check comes before a slot is taken: with every slot held, a run
/// whose image is not stored is still told why.
#[tokio::test]
async fn the_image_refusal_comes_before_the_slot() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let slots = state.agent_semaphore.available_permits();
    let _held = std::sync::Arc::clone(&state.agent_semaphore)
        .try_acquire_many_owned(u32::try_from(slots).unwrap())
        .unwrap();
    let gone = AttachmentId::of(b"never uploaded").to_string();

    assert_eq!(
        put(&app, local(&messages(&[], &[]))).await,
        (StatusCode::TOO_MANY_REQUESTS, "agent_busy".to_owned())
    );
    assert_eq!(
        put(&app, local(&messages(&[], &[&gone]))).await,
        refusal("attachment_not_found")
    );
}
