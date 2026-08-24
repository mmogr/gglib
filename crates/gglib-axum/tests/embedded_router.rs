//! Contract tests for the router that every shipped daemon actually builds.
//!
//! `crates/gglib-axum/src/ui_tests.rs` covers [`respond`] in isolation — one
//! path in, one response out. Nothing covered how that handler *composes* with
//! the `/api` nest and the Host guard, and that gap is exactly how the
//! SPA-swallows-`/api` defect reached `main`: every assertion held, because
//! none of them built a router.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

use std::sync::Arc;

use common::harness::{test_access, test_state};
use gglib_core::CorsConfig;

/// The embedded router is the one every shipped daemon now builds, and until
/// this test nothing constructed it — every unit test called `respond` directly
/// and so never exercised how the fallback composes with the `/api` nest. That
/// gap is exactly how the SPA-swallows-`/api` defect reached `main`.
///
/// Companion to [`events_endpoint_not_intercepted_by_spa_fallback`], which
/// guards the same class of bug on the directory-backed router.
#[tokio::test]
async fn embedded_router_does_not_swallow_api_paths() {
    let state = test_state(CorsConfig::AllowAll).await;
    let app = gglib_axum::create_embedded_spa_router(
        Arc::clone(&state),
        &CorsConfig::AllowAll,
        test_access(),
    );

    for uri in ["/api", "/api/nonexistent", "/api/proxy/statuss"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .header("Host", "127.0.0.1:9887")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{uri} must 404 rather than fall through to the dashboard shell"
        );
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        assert!(
            !content_type.starts_with("text/html"),
            "{uri} answered {content_type}, i.e. the SPA shell"
        );
    }
}

/// The Host guard is layered *after* the fallback precisely so it wraps the
/// embedded assets too — a DNS-rebound page must not be able to load the
/// dashboard at all. That ordering was asserted only in a comment.
#[tokio::test]
async fn the_host_guard_covers_embedded_assets() {
    let state = test_state(CorsConfig::AllowAll).await;
    let app = gglib_axum::create_embedded_spa_router(
        Arc::clone(&state),
        &CorsConfig::AllowAll,
        test_access(),
    );

    // Positive control first, so the negative case below cannot pass because
    // the shell is simply unreachable in this build.
    let allowed = app
        .clone()
        .oneshot(
            Request::builder()
                .header("Host", "127.0.0.1:9887")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        allowed.status(),
        StatusCode::OK,
        "the loopback Host should be served the shell — otherwise this test proves nothing"
    );

    let rebound = app
        .oneshot(
            Request::builder()
                .header("Host", "evil.example.com")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(
        rebound.status(),
        StatusCode::OK,
        "a rebound Host must not be served the dashboard shell"
    );
}
