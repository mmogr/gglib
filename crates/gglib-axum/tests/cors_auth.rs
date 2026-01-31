//! Integration tests for CORS preflight and bearer token authentication.
//!
//! Verifies that the embedded API server correctly handles:
//! - OPTIONS preflight requests with proper CORS headers
//! - Bearer token authentication on /api/* endpoints
//! - Unauthenticated access to /health endpoint

mod common;

use gglib_axum::{
    bootstrap::{CorsConfig, ServerConfig, bootstrap},
    embedded::{EmbeddedServerConfig, default_embedded_cors_origins, start_embedded_server},
};
use reqwest::{Method, StatusCode, header};
use common::ports::TEST_BASE_PORT;

/// Helper to create a test config that doesn't require llama-server.
fn test_config() -> ServerConfig {
    ServerConfig {
        port: 0,
        base_port: TEST_BASE_PORT,
        llama_server_path: "/nonexistent/llama-server".into(),
        max_concurrent: 1,
        static_dir: None,
        cors: CorsConfig::AllowAll,
    }
}

/// Helper to create test server and return (base_url, token)
async fn setup_test_server() -> (String, String) {
    let ctx = bootstrap(test_config()).await.expect("Failed to bootstrap");

    let server_config = EmbeddedServerConfig {
        cors_origins: default_embedded_cors_origins(),
    };

    let (info, _handle) = start_embedded_server(ctx, server_config)
        .await
        .expect("Failed to start embedded server");

    let base_url = format!("http://127.0.0.1:{}", info.port);
    (base_url, info.token)
}

#[tokio::test]
async fn test_health_endpoint_no_auth_required() {
    let (base_url, _token) = setup_test_server().await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "/health should be accessible without authentication"
    );
}

#[tokio::test]
async fn test_api_endpoint_requires_auth() {
    let (base_url, _token) = setup_test_server().await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/models", base_url))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "/api/models should return 401 without token"
    );

    // Verify WWW-Authenticate header is present
    assert!(
        response.headers().contains_key(header::WWW_AUTHENTICATE),
        "Response should include WWW-Authenticate header"
    );
}

#[tokio::test]
async fn test_api_endpoint_with_valid_token() {
    let (base_url, token) = setup_test_server().await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/models", base_url))
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "/api/models should return 200 with valid token"
    );
}

#[tokio::test]
async fn test_cors_preflight_request() {
    let (base_url, _token) = setup_test_server().await;

    let client = reqwest::Client::new();
    let response = client
        .request(Method::OPTIONS, format!("{}/api/models", base_url))
        .header(header::ORIGIN, "http://tauri.localhost")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .header(
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "authorization, content-type",
        )
        .send()
        .await
        .expect("Failed to send preflight request");

    // Accept both 200 and 204 for preflight (some CORS middleware uses 204)
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::NO_CONTENT,
        "Preflight should return 200 or 204, got: {}",
        response.status()
    );

    // Verify CORS headers are present
    let allow_origin = response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN);
    assert!(
        allow_origin.is_some(),
        "Preflight response should include Access-Control-Allow-Origin"
    );

    let allow_headers = response
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    assert!(
        allow_headers.to_lowercase().contains("authorization"),
        "CORS should allow Authorization header, got: {}",
        allow_headers
    );

    assert!(
        allow_headers.to_lowercase().contains("content-type"),
        "CORS should allow Content-Type header, got: {}",
        allow_headers
    );
}

#[tokio::test]
async fn test_cors_actual_request_with_origin() {
    let (base_url, token) = setup_test_server().await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/models", base_url))
        .header(header::ORIGIN, "http://tauri.localhost")
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Actual request with valid origin and token should succeed"
    );

    // Verify CORS response headers
    let allow_origin = response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN);
    assert!(
        allow_origin.is_some(),
        "Response should include Access-Control-Allow-Origin"
    );
}

#[tokio::test]
async fn test_invalid_token_rejected() {
    let (base_url, _token) = setup_test_server().await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/api/models", base_url))
        .header(header::AUTHORIZATION, "Bearer invalid-token-12345")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Invalid token should be rejected"
    );
}

#[tokio::test]
async fn test_malformed_auth_header_rejected() {
    let (base_url, token) = setup_test_server().await;

    let client = reqwest::Client::new();

    // Missing "Bearer" prefix
    let response = client
        .get(format!("{}/api/models", base_url))
        .header(header::AUTHORIZATION, &token)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Token without Bearer prefix should be rejected"
    );

    // Wrong auth scheme
    let response = client
        .get(format!("{}/api/models", base_url))
        .header(header::AUTHORIZATION, format!("Basic {}", token))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Non-Bearer auth scheme should be rejected"
    );
}
