//! The far machine's chats, runs and models, on the router a shipped daemon
//! builds, when this machine is joined to nothing: every route is a `409`
//! that says `gglib remote join`, and nothing is forwarded anywhere.

mod common;

use axum::body::Body;
use axum::http::{Method, StatusCode};

use common::origin::{HOST, authed, send_request, shipped, shipped_cors};
use gglib_axum::DaemonAccess;
use gglib_core::contracts::http::daemon;

/// Every route the far-machine contract names, with every verb it is called
/// with. Derived from the contract, so a route added there is swept here.
#[tokio::test]
async fn every_far_route_is_a_409_when_not_joined() {
    let app = shipped(&shipped_cors(), DaemonAccess::loopback()).await;
    let routes: Vec<_> = daemon::remote_route_contract()
        .into_iter()
        .flat_map(|(methods, path)| methods.iter().map(move |m| (*m, path.clone())))
        .collect();
    assert!(routes.len() >= 9, "the sweep found only {routes:?}");

    for (method, path) in routes {
        let method = Method::from_bytes(method.as_bytes()).unwrap();
        // A turn's body, which every other route here reads nothing of.
        let request = authed()
            .method(method.clone())
            .uri(&path)
            .header("host", HOST)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"content":"hi"}"#))
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
