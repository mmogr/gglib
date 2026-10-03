//! The body limit on the two routes that take images, over real HTTP.
//!
//! Both take 32 MiB in place of axum's 2 MiB default, and both refuse more
//! with a code: left to axum, `POST /v1/chat/completions` answers a line of
//! plain text, and `PUT /v1/runs/{id}` that the body is not a JSON object.

mod fixtures;

use std::sync::Arc;

use gglib_core::contracts::http::MAX_BODY_BYTES;
use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use fixtures::common::{CountingRuntime, spawn_proxy_with_catalog};
use fixtures::images::{SEES, Sight, png_url, request, spawn_upstream, user_with_image};
use fixtures::runs::{FakeRuns, code, json as answer, serve};

const MIB: usize = 1024 * 1024;

/// A JSON object exactly `len` bytes long.
fn object_of(len: usize) -> String {
    let frame = r#"{"model":"sees","pad":""}"#.len();
    format!(r#"{{"model":"sees","pad":"{}"}}"#, "x".repeat(len - frame))
}

fn assert_too_large(status: StatusCode, body: &Value) {
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    assert_eq!(code(body), "request_too_large");
    assert_eq!(body["error"]["type"], "invalid_request_error");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("32 MiB"),
        "the message names the limit: {message}"
    );
}

#[test]
fn the_limit_is_32_mib() {
    assert_eq!(MAX_BODY_BYTES, 32 * MIB);
    assert_eq!(object_of(MAX_BODY_BYTES).len(), MAX_BODY_BYTES);
}

#[tokio::test]
async fn a_chat_completion_of_3_mib_is_taken_and_one_over_the_limit_is_refused_with_a_code() {
    let cancel = CancellationToken::new();
    let (port, upstream) = spawn_upstream(cancel.clone()).await;
    let (runtime, _) = CountingRuntime::new(port, SEES);
    let (base, proxy_cancel) = spawn_proxy_with_catalog(
        runtime as Arc<dyn ModelRuntimePort>,
        Arc::new(Sight) as Arc<dyn ModelCatalogPort>,
    )
    .await;
    let send = |body: String| {
        Client::new()
            .post(format!("{base}/v1/chat/completions"))
            .header("content-type", "application/json")
            .body(body)
            .send()
    };

    let image = request(SEES, &[user_with_image("what is this?", &png_url(3 * MIB))]);
    let image = serde_json::to_string(&image).unwrap();
    assert!(image.len() > 3 * MIB);
    let taken = send(image).await.unwrap();
    assert_eq!(
        taken.status(),
        StatusCode::OK,
        "{}",
        taken.text().await.unwrap()
    );

    let at_the_limit = send(object_of(MAX_BODY_BYTES)).await.unwrap();
    assert_ne!(at_the_limit.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let (status, body) = answer(send(object_of(MAX_BODY_BYTES + 1)).await.unwrap()).await;
    assert_too_large(status, &body);
    assert_eq!(
        upstream.seen.lock().unwrap().len(),
        2,
        "the refused body went nowhere"
    );

    proxy_cancel.cancel();
    cancel.cancel();
}

#[tokio::test]
async fn a_run_of_3_mib_is_taken_and_one_over_the_limit_is_refused_with_a_code() {
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&runs))).await;
    let put = |id: &str, body: String| {
        Client::new()
            .put(format!("{base}/v1/runs/{id}"))
            .header("content-type", "application/json")
            .body(body)
            .send()
    };

    let taken = put("r1", object_of(3 * MIB)).await.unwrap();
    assert_eq!(taken.status(), StatusCode::CREATED);

    let at_the_limit = put("r2", object_of(MAX_BODY_BYTES)).await.unwrap();
    assert_eq!(at_the_limit.status(), StatusCode::CREATED);

    let (status, body) = answer(put("r3", object_of(MAX_BODY_BYTES + 1)).await.unwrap()).await;
    assert_too_large(status, &body);
    assert_eq!(runs.scopes().len(), 2, "the refused body started no run");
    cancel.cancel();
}

/// A body that is not JSON is still told so, and not that it was too large.
#[tokio::test]
async fn a_run_whose_body_is_not_json_keeps_its_own_refusal() {
    let runs = Arc::new(FakeRuns::default());
    let (base, cancel) = serve(Some(Arc::clone(&runs))).await;
    let put = |content_type: &str, body: &'static str| {
        Client::new()
            .put(format!("{base}/v1/runs/r1"))
            .header("content-type", content_type)
            .body(body)
            .send()
    };

    for response in [
        put("application/json", "{not json").await.unwrap(),
        put("text/plain", "{}").await.unwrap(),
    ] {
        let (status, body) = answer(response).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(code(&body), "invalid_request");
        assert_eq!(
            body["error"]["message"],
            "a run's request body must be a JSON object, sent as application/json"
        );
    }
    assert!(runs.scopes().is_empty());
    cancel.cancel();
}
