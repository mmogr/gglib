//! The projector link over the daemon's HTTP routes: the picker's choices
//! and the update key are wired to their handlers.

mod common;

use axum::body::Body;
use axum::http::StatusCode;
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::test_app;
use common::origin::authed;
use gglib_core::CorsConfig;

/// The picker's route reaches its handler: an unknown id is the handler's
/// JSON 404, not the fallback's.
#[tokio::test]
async fn the_picker_route_is_wired_and_404s_on_an_unknown_id() {
    let app = test_app(CorsConfig::AllowAll).await;

    let response = app
        .oneshot(
            authed()
                .header("Host", "127.0.0.1:9887")
                .uri("/api/models/999999/projectors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).expect("a JSON error body");
    assert!(json.to_string().contains("999999"), "{json}");
}

/// An update reads `projectorPath` as a path and as `null`: both reach the
/// model lookup, which is where an unknown id stops them.
#[tokio::test]
async fn an_update_accepts_a_projector_path_and_null() {
    for body in [
        r#"{"projectorPath": "/models/mmproj-F16.gguf"}"#,
        r#"{"projectorPath": null}"#,
    ] {
        let response = test_app(CorsConfig::AllowAll)
            .await
            .oneshot(
                authed()
                    .header("Host", "127.0.0.1:9887")
                    .method("PUT")
                    .uri("/api/models/999999")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{body}");
    }
}
