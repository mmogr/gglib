//! `POST /api/config/system/update-llama`: the GUI's update is refused when
//! `gglib config llama update` is, and in the same words.
//!
//! Both ask `gglib_runtime::llama::update_preflight`. The command's own test,
//! in `gglib-cli`, compares what it prints with the same `UpdateRefusal`
//! values this one compares the stream's `failed` event with.
//!
//! The data root is this test binary's own (`isolate_data_root`), and every
//! state laid out in it is one the preflight refuses, so nothing is pulled or
//! built. A checkout with local changes is not refused, so what the route
//! says of one is compared beside the handler, in `setup_tests.rs`, where the
//! update is a stand-in.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, StatusCode};
use gglib_core::CorsConfig;
use gglib_runtime::llama::UpdateRefusal;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use common::harness::test_app;
use common::origin::authed;

/// Ask for an update and return the events the stream carried, in order.
async fn update_events(app: &Router) -> Vec<Value> {
    let request = authed()
        .method(Method::POST)
        .uri("/api/config/system/update-llama")
        .header("Host", "127.0.0.1:9887")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|data| serde_json::from_str(data.trim()).expect("each event is JSON"))
        .collect()
}

/// The one event a refused update streams: `failed`, with the refusal.
fn refusal_in(events: &[Value]) -> &str {
    assert_eq!(
        events.len(),
        1,
        "a refused update streams nothing else: {events:?}"
    );
    assert_eq!(events[0]["type"], "failed", "{events:?}");
    events[0]["message"].as_str().expect("a message")
}

#[tokio::test]
async fn the_route_refuses_an_update_as_the_command_does() {
    let app = test_app(CorsConfig::AllowAll).await;
    let server = gglib_core::paths::llama_server_path().unwrap();
    assert!(!server.exists(), "the test's own data root starts empty");

    // Nothing installed.
    let events = update_events(&app).await;
    assert_eq!(refusal_in(&events), UpdateRefusal::NotInstalled.to_string());

    // A binary and no source checkout: what a pre-built download leaves.
    // The route once let this state past a check for the binary it never
    // made, and gave different advice from the command's.
    std::fs::create_dir_all(server.parent().unwrap()).unwrap();
    std::fs::write(&server, "not a program").unwrap();
    let events = update_events(&app).await;
    assert_eq!(
        refusal_in(&events),
        UpdateRefusal::NoSourceCheckout.to_string()
    );
    assert!(refusal_in(&events).contains("'gglib config llama rebuild'"));
}
