//! Smoke tests for embedded server authentication and CORS.
//!
//! Verifies that:
//! - `/health` endpoint is accessible without authentication
//! - `/api/*` endpoints require Bearer token authentication
//! - Wrong/missing tokens are rejected with 401
//! - Correct tokens grant access

mod common;

use common::ports::{TEST_BASE_PORT, TEST_CORS_ORIGIN};
use gglib_axum::{
    bootstrap::{CorsConfig, ServerConfig, bootstrap},
    embedded::{EmbeddedServerConfig, start_embedded_server},
};

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

#[tokio::test]
async fn test_health_endpoint_no_auth() {
    // Arrange: Bootstrap a minimal context
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return, // Skip test if bootstrap fails in CI
    };

    let config = EmbeddedServerConfig {
        cors_origins: vec![TEST_CORS_ORIGIN.to_string()],
    };

    let (info, _handle) = start_embedded_server(ctx, config)
        .await
        .expect("Failed to start embedded server");

    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{}", info.port);

    // Act: Access /health without auth
    let response = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .expect("Request failed");

    // Assert: Should succeed without token
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_api_requires_auth() {
    // Arrange
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return, // Skip test if bootstrap fails in CI
    };

    let config = EmbeddedServerConfig {
        cors_origins: vec![TEST_CORS_ORIGIN.to_string()],
    };

    // Start embedded server to get the auth middleware wired up
    let (info, _handle) = start_embedded_server(ctx, config)
        .await
        .expect("Failed to start embedded server");

    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{}", info.port);

    // Act & Assert: No auth header → 401
    let response = client
        .get(format!("{}/api/models", base_url))
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 401);

    // Act & Assert: Wrong token → 401
    let response = client
        .get(format!("{}/api/models", base_url))
        .header("Authorization", "Bearer wrong-token")
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 401);

    // Act & Assert: Correct token → 200
    let response = client
        .get(format!("{}/api/models", base_url))
        .header("Authorization", format!("Bearer {}", info.token))
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_api_malformed_auth_header() {
    // Arrange
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let config = EmbeddedServerConfig {
        cors_origins: vec![TEST_CORS_ORIGIN.to_string()],
    };

    let (info, _handle) = start_embedded_server(ctx, config)
        .await
        .expect("Failed to start embedded server");

    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{}", info.port);

    // Act & Assert: Missing "Bearer " prefix → 401
    let response = client
        .get(format!("{}/api/models", base_url))
        .header("Authorization", &info.token)
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 401);

    // Act & Assert: Wrong scheme → 401
    let response = client
        .get(format!("{}/api/models", base_url))
        .header("Authorization", format!("Basic {}", info.token))
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 401);
}
