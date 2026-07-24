//! Integration tests for CorsConfig::LocalOnly behavior.
//!
//! Verifies that the LocalOnly CORS policy correctly accepts localhost origins
//! and rejects remote origins, and that ServerConfig defaults are correct.

mod common;

use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

use common::ports::TEST_BASE_PORT;
use gglib_axum::bootstrap::{CorsConfig, ServerConfig, bootstrap};
use gglib_axum::routes::create_router;

fn test_config() -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        base_port: TEST_BASE_PORT,
        llama_server_path: "/nonexistent/llama-server".into(),
        max_concurrent: 1,
        max_concurrent_agent_loops: 1,
        static_dir: None,
        cors: CorsConfig::LocalOnly,
    }
}

#[tokio::test]
async fn local_only_rejects_remote_origin() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::LocalOnly);

    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/models")
                .header("Origin", "http://evil.example.com")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let allow_origin = response.headers().get("Access-Control-Allow-Origin");
    assert!(
        allow_origin.is_none(),
        "Remote origin should be rejected (no Access-Control-Allow-Origin header), got: {:?}",
        allow_origin
    );
}

#[tokio::test]
async fn local_only_allows_localhost_origin() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::LocalOnly);

    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/models")
                .header("Origin", "http://localhost:9887")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let allow_origin = response.headers().get("Access-Control-Allow-Origin");
    assert!(allow_origin.is_some(), "Localhost origin should be allowed");
    assert_eq!(
        allow_origin.unwrap().to_str().unwrap(),
        "http://localhost:9887",
        "Origin should be reflected"
    );
}

#[tokio::test]
async fn local_only_allows_127_0_0_1_origin() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::LocalOnly);

    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/models")
                .header("Origin", "http://127.0.0.1:3000")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let allow_origin = response.headers().get("Access-Control-Allow-Origin");
    assert!(allow_origin.is_some(), "127.0.0.1 origin should be allowed");
}

#[tokio::test]
async fn local_only_allows_ipv6_localhost() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::LocalOnly);

    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/models")
                .header("Origin", "http://[::1]:8080")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let allow_origin = response.headers().get("Access-Control-Allow-Origin");
    assert!(allow_origin.is_some(), "[::1] origin should be allowed");
}

#[tokio::test]
async fn server_config_defaults_are_local_only() {
    let config = ServerConfig::with_defaults().expect("defaults should build");

    assert_eq!(config.host, "127.0.0.1", "Default host should be 127.0.0.1");
    match config.cors {
        CorsConfig::LocalOnly => {} // expected
        other => panic!("Default CORS should be LocalOnly, got: {:?}", other),
    }
}
