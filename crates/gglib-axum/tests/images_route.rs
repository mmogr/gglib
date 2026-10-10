//! `/api/images/generations` on the router a daemon builds: the proxy's
//! handler over the daemon's own image driver. With nothing in the library,
//! drawing is unavailable and says why, before anything queues.
//! `/api/images/drawing` beside it says so before anything is sent.
//!
//! That the route asks for the daemon's token is `daemon_token_door`'s
//! sweep, which walks it with every other `/api` route.

mod common;

use axum::body::Body;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::test_app;
use common::origin::{HOST, authed};
use gglib_core::CorsConfig;

#[tokio::test]
async fn an_empty_library_has_nothing_to_draw_with() {
    let app = test_app(CorsConfig::LocalOnly).await;
    let response = app
        .oneshot(
            authed()
                .method("POST")
                .uri("/api/images/generations")
                .header("host", HOST)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"prompt":"a lighthouse"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "drawing_unavailable");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no image model"),
        "{body}"
    );
}

async fn drawing(query: &str) -> serde_json::Value {
    let app = test_app(CorsConfig::LocalOnly).await;
    let response = app
        .oneshot(
            authed()
                .uri(format!("/api/images/drawing{query}"))
                .header("host", HOST)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

/// The Draw button's answer: with no image runtime here, unavailable and
/// why; a far chat's model is refused before anything here is asked.
#[tokio::test]
async fn the_drawing_route_says_why_drawing_is_unavailable() {
    let here = drawing("").await;
    assert_eq!(here["available"], false, "{here}");
    assert_eq!(here["code"], "drawing_unavailable");
    assert!(
        here["reason"].as_str().unwrap().contains("not installed"),
        "{here}"
    );

    let far = drawing("?far=true&calls_tools=true").await;
    assert!(
        far["reason"].as_str().unwrap().contains("another machine"),
        "{far}"
    );
}
