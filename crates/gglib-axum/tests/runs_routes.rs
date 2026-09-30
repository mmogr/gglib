//! `/api/runs/*` on the daemon's router, over the real registry.
//!
//! The harness never starts a proxy, so every chat run here fails with
//! `proxy_not_running`, which is itself one of the rules: a run does not
//! start the proxy. The run's life against a proxy is `gglib-app-services`'
//! `runs/chat_tests.rs`.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, StatusCode};
use gglib_core::CorsConfig;
use gglib_core::contracts::http::daemon::{RUNS_PATH, run_cancel_path, run_events_path, run_path};
use gglib_core::domain::runs::{RunInfo, RunList, RunStatus};
use gglib_core::ports::{RunScope, RunsPort};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use common::harness::test_state_and_app;
use common::origin::authed;

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, String) {
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
    let bytes = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        response.into_body().collect(),
    )
    .await
    .expect("the body ends")
    .unwrap()
    .to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn chat() -> Value {
    json!({ "model": "qwen", "messages": [{ "role": "user", "content": "hi" }] })
}

#[tokio::test]
async fn a_put_starts_a_run_and_a_repeat_answers_with_it() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;

    let (status, body) = call(&app, Method::PUT, &run_path("r-1"), Some(chat())).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let first: RunInfo = serde_json::from_str(&body).unwrap();
    assert_eq!(first.id, "r-1");
    assert_eq!(first.model.as_deref(), Some("qwen"));

    let (status, body) = call(&app, Method::PUT, &run_path("r-1"), Some(chat())).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let again: RunInfo = serde_json::from_str(&body).unwrap();
    assert_eq!(
        (again.id, again.created_at_ms),
        ("r-1".into(), first.created_at_ms)
    );
}

#[tokio::test]
async fn a_run_with_no_proxy_running_ends_failed_and_the_stream_says_so() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    call(&app, Method::PUT, &run_path("r-1"), Some(chat())).await;

    let (status, body) = call(&app, Method::GET, &run_events_path("r-1", 0), None).await;

    assert_eq!(status, StatusCode::OK);
    let data = body
        .strip_prefix("event: run\ndata: ")
        .and_then(|rest| rest.strip_suffix("\n\n"))
        .unwrap_or_else(|| panic!("one final run event: {body:?}"));
    let info: RunInfo = serde_json::from_str(data).unwrap();
    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(
        info.error.map(|e| e.code).as_deref(),
        Some("proxy_not_running")
    );

    let (status, body) = call(&app, Method::GET, &run_path("r-1"), None).await;
    assert_eq!(status, StatusCode::OK);
    let read: RunInfo = serde_json::from_str(&body).unwrap();
    assert_eq!(read.status, RunStatus::Failed);
}

#[tokio::test]
async fn the_list_is_newest_first_and_cancel_answers_with_the_run() {
    let (_, app) = test_state_and_app(CorsConfig::AllowAll).await;
    for id in ["older", "newer"] {
        call(&app, Method::PUT, &run_path(id), Some(chat())).await;
    }

    let (status, body) = call(&app, Method::GET, RUNS_PATH, None).await;
    assert_eq!(status, StatusCode::OK);
    let list: RunList = serde_json::from_str(&body).unwrap();
    let ids: Vec<&str> = list.runs.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["newer", "older"]);

    let (status, body) = call(&app, Method::POST, &run_cancel_path("older"), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let info: RunInfo = serde_json::from_str(&body).unwrap();
    assert_eq!(info.id, "older");
    assert!(info.status.is_terminal());
}

#[tokio::test]
async fn refusals_use_the_daemons_error_body_with_the_runs_code() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    state
        .runs
        .create(RunScope::Device("phone".into()), "phones", chat())
        .unwrap();

    let cases = [
        (
            Method::PUT,
            run_path("not.an.id"),
            Some(chat()),
            400,
            "invalid_request",
        ),
        (
            Method::PUT,
            run_path("r-2"),
            Some(json!([1, 2])),
            400,
            "invalid_request",
        ),
        (Method::GET, run_path("nobody"), None, 404, "not_found"),
        (
            Method::POST,
            run_cancel_path("nobody"),
            None,
            404,
            "not_found",
        ),
        (
            Method::GET,
            run_events_path("nobody", 0),
            None,
            404,
            "not_found",
        ),
        (
            Method::GET,
            run_events_path("phones", 0),
            None,
            403,
            "not_yours",
        ),
        (
            Method::PUT,
            run_path("phones"),
            Some(chat()),
            409,
            "conflict",
        ),
    ];
    for (method, uri, body, status, code) in cases {
        let (got, text) = call(&app, method.clone(), &uri, body).await;
        assert_eq!(got.as_u16(), status, "{method} {uri}: {text}");
        let error: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(error["type"], json!(code), "{method} {uri}: {text}");
        assert_eq!(error["status"], json!(status));
        assert!(error["error"].is_string());
    }
}

#[tokio::test]
async fn this_machine_sees_and_cancels_a_devices_run() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    state
        .runs
        .create(RunScope::Device("phone".into()), "phones", chat())
        .unwrap();

    let (status, body) = call(&app, Method::GET, RUNS_PATH, None).await;
    assert_eq!(status, StatusCode::OK);
    let list: RunList = serde_json::from_str(&body).unwrap();
    assert_eq!(list.runs[0].device.as_deref(), Some("phone"));
    let (status, _) = call(&app, Method::POST, &run_cancel_path("phones"), None).await;
    assert_eq!(status, StatusCode::OK);
}

/// An error's message can quote a device's reply, so this machine's routes
/// answer a device's failed run with its code and fixed text.
#[tokio::test]
async fn a_devices_error_text_stays_with_the_device() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let phone = RunScope::Device("phone".into());
    state.runs.create(phone.clone(), "phones", chat()).unwrap();
    let own = loop {
        let info = state.runs.get(&phone, "phones").unwrap();
        if info.status.is_terminal() {
            break info;
        }
        tokio::task::yield_now().await;
    };
    let real = own.error.expect("it failed").message;
    assert!(real.contains("proxy is not running"), "{real}");

    let (_, one) = call(&app, Method::GET, &run_path("phones"), None).await;
    let (_, all) = call(&app, Method::GET, RUNS_PATH, None).await;
    let (_, cancelled) = call(&app, Method::POST, &run_cancel_path("phones"), None).await;
    for body in [one, all, cancelled] {
        assert!(
            body.contains("proxy_not_running"),
            "the code is kept: {body}"
        );
        assert!(
            !body.contains(&real),
            "the device's text reached this machine: {body}"
        );
    }
}

/// The service graph hands the daemon's registry to its proxy, so the door a
/// paired device reaches serves these same runs.
#[tokio::test]
async fn the_proxy_is_handed_the_daemons_runs() {
    let (state, _) = test_state_and_app(CorsConfig::AllowAll).await;
    let phone = RunScope::Device("phone".into());
    state.runs.create(phone.clone(), "phones", chat()).unwrap();

    let bound = state.proxy.runs().expect("the graph binds the runs");
    assert_eq!(bound.list(&phone).runs[0].id, "phones");
}
