//! `PUT /api/runs/{id}?kind=agent` on the daemon's router: each refusal
//! comes with a code and leaves no run behind.
//!
//! The harness runs no llama-server, so a request that passes every check
//! of its own is refused where `/api/agent/chat` refuses it: the port. The
//! run's life over a loop is `handlers/agent/run_tests.rs`.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, StatusCode};
use gglib_core::CorsConfig;
use gglib_core::contracts::http::daemon::{RUNS_PATH, run_path};
use gglib_core::domain::runs::{RunInfo, RunKind, RunList};
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

fn agent(id: &str) -> String {
    format!("{}?kind=agent", run_path(id))
}

fn chat_request() -> Value {
    json!({ "port": 9000, "messages": [{ "role": "user", "content": "PROMPT-SECRET" }] })
}

async fn no_runs(app: &Router) {
    let (_, list) = call(app, Method::GET, RUNS_PATH, None).await;
    let list: RunList = serde_json::from_value(list).unwrap();
    assert!(list.runs.is_empty(), "{list:?}");
}

#[tokio::test]
async fn an_unknown_conversation_is_a_404_and_nothing_runs() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let mut body = chat_request();
    body["conversation_id"] = json!(4242);

    let (status, error) = call(&app, Method::PUT, &agent("a1"), Some(body)).await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{error}");
    assert_eq!(error["type"], "conversation_not_found");
    no_runs(&app).await;
}

#[tokio::test]
async fn with_every_agent_slot_taken_it_is_a_429_and_nothing_runs() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let slots = state.agent_semaphore.available_permits();
    let _held = std::sync::Arc::clone(&state.agent_semaphore)
        .try_acquire_many_owned(u32::try_from(slots).unwrap())
        .unwrap();

    let (status, error) = call(&app, Method::PUT, &agent("a1"), Some(chat_request())).await;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{error}");
    assert_eq!(error["type"], "agent_busy");
    no_runs(&app).await;
}

/// The same check, and the same words, as the chat route: the code is
/// shared, not copied.
#[tokio::test]
async fn a_request_the_chat_route_refuses_is_refused_alike_with_a_code() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let (chat_status, chat_error) =
        call(&app, Method::POST, "/api/agent/chat", Some(chat_request())).await;

    let (status, error) = call(&app, Method::PUT, &agent("a1"), Some(chat_request())).await;

    assert_eq!(
        (chat_status, status),
        (StatusCode::BAD_REQUEST, StatusCode::BAD_REQUEST)
    );
    assert_eq!(error["error"], chat_error["error"], "{error}");
    assert_eq!(error["type"], "invalid_request");
    no_runs(&app).await;
}

#[tokio::test]
async fn a_body_that_is_not_an_agent_request_is_refused_without_quoting_it() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let body = json!({ "port": "BODY-SECRET", "messages": [] });

    let (status, error) = call(&app, Method::PUT, &agent("a1"), Some(body)).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert_eq!(error["type"], "invalid_request");
    assert!(!error.to_string().contains("BODY-SECRET"), "{error}");
    no_runs(&app).await;
}

/// A repeat answers with the run that has the id before anything is
/// checked, as a chat run's repeat does, so a client that lost the answer
/// can ask again even while the run holds the last slot.
#[tokio::test]
async fn a_repeated_id_answers_with_its_run_before_any_check() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let chat = json!({ "model": "qwen", "messages": [] });
    let (status, _) = call(&app, Method::PUT, &run_path("r1"), Some(chat)).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, info) = call(&app, Method::PUT, &agent("r1"), Some(chat_request())).await;

    assert_eq!(status, StatusCode::OK, "{info}");
    let info: RunInfo = serde_json::from_value(info).unwrap();
    assert_eq!((info.id.as_str(), info.kind), ("r1", RunKind::Chat));
}

#[tokio::test]
async fn an_unknown_kind_is_refused() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let uri = format!("{}?kind=essay", run_path("a1"));

    let (status, error) = call(&app, Method::PUT, &uri, Some(chat_request())).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert_eq!(error["type"], "invalid_request");
    no_runs(&app).await;
}

/// `kind=chat` is the chat path, with a chat request's body.
#[tokio::test]
async fn kind_chat_starts_a_chat_run() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let uri = format!("{}?kind=chat", run_path("c1"));
    let chat = json!({ "model": "qwen", "messages": [] });

    let (status, info) = call(&app, Method::PUT, &uri, Some(chat)).await;

    assert_eq!(status, StatusCode::CREATED, "{info}");
    let info: RunInfo = serde_json::from_value(info).unwrap();
    assert_eq!(info.kind, RunKind::Chat);
}
