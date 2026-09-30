//! The far machine's chats and runs, on the router a shipped daemon builds,
//! when this machine is joined to nothing: every route is a `409` that says
//! `gglib remote join`, and nothing is forwarded anywhere.

mod common;

use axum::body::Body;
use axum::http::{Method, StatusCode};

use common::origin::{HOST, authed, send_request, shipped, shipped_cors};
use gglib_axum::DaemonAccess;
use gglib_core::contracts::http::daemon;

#[tokio::test]
async fn every_far_chat_route_is_a_409_when_not_joined() {
    let app = shipped(&shipped_cors(), DaemonAccess::loopback()).await;

    for (method, path, body) in [
        (Method::GET, daemon::REMOTE_CHATS_PATH.to_owned(), ""),
        (Method::GET, daemon::remote_chat_path(12), ""),
        (
            Method::PUT,
            daemon::remote_turn_path(12, "chat-1"),
            r#"{"content":"hi"}"#,
        ),
        (Method::GET, daemon::REMOTE_RUNS_PATH.to_owned(), ""),
        (Method::GET, daemon::remote_run_events_path("chat-1", 0), ""),
        (Method::POST, daemon::remote_run_cancel_path("chat-1"), ""),
    ] {
        let request = authed()
            .method(method.clone())
            .uri(&path)
            .header("host", HOST)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let answer = send_request(&app, request).await;

        assert_eq!(
            answer.status,
            StatusCode::CONFLICT,
            "{method} {path}: {}",
            answer.body
        );
        assert!(
            answer.body.contains("`gglib remote join`"),
            "{method} {path}: {}",
            answer.body
        );
    }
}

/// A turn is its text alone: a body that also carries history is refused
/// before this machine's connection is even looked at.
#[tokio::test]
async fn a_turn_that_carries_history_is_refused() {
    let app = shipped(&shipped_cors(), DaemonAccess::loopback()).await;
    let request = authed()
        .method(Method::PUT)
        .uri(daemon::remote_turn_path(12, "chat-1"))
        .header("host", HOST)
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"content":"hi","messages":[{"role":"user","content":"old"}]}"#,
        ))
        .unwrap();

    let answer = send_request(&app, request).await;

    assert_eq!(
        answer.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        answer.body
    );
}
