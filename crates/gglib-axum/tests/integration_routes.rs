//! Integration tests for the Axum web server.
//!
//! These tests verify that routes are correctly wired to handlers.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::ports::{TEST_BASE_PORT, TEST_MODEL_PORT};
use gglib_axum::bootstrap::{CorsConfig, ServerConfig, bootstrap};
use gglib_axum::routes::create_router;

/// Helper to create a test config that doesn't require llama-server.
fn test_config() -> ServerConfig {
    ServerConfig {
        port: 0, // Not used in tests
        base_port: TEST_BASE_PORT,
        llama_server_path: "/nonexistent/llama-server".into(),
        max_concurrent: 1,
        static_dir: None,
        cors: CorsConfig::AllowAll,
    }
}

#[tokio::test]
async fn health_endpoint_returns_ok() {
    // Skip if bootstrap fails (e.g., no DB path available in CI)
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return, // Skip test if bootstrap fails
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"OK");
}

#[tokio::test]
async fn models_endpoint_returns_json() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify it returns valid JSON array (may contain models in test environment)
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(
        body_str.starts_with('[') && body_str.ends_with(']'),
        "Expected JSON array, got: {}",
        body_str
    );
}

#[tokio::test]
async fn servers_endpoint_returns_json_array() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/servers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"[]");
}

#[tokio::test]
async fn downloads_endpoint_returns_queue_snapshot() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/downloads")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body).unwrap();
    // Should contain queue snapshot fields
    assert!(body_str.contains("items"));
    assert!(body_str.contains("max_size"));
}

#[tokio::test]
async fn events_endpoint_returns_sse_stream() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // SSE endpoint should return 200 with text/event-stream content type
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or("").starts_with("text/event-stream"))
            .unwrap_or(false)
    );
}

/// Regression test: SSE endpoint should NOT be intercepted by SPA fallback.
/// This catches the bug where /api/events returns HTML instead of event-stream.
#[tokio::test]
async fn events_endpoint_not_intercepted_by_spa_fallback() {
    use gglib_axum::routes::create_spa_router;
    use std::io::Write;
    use tempfile::TempDir;

    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Create a temp directory with an index.html (SPA fallback target)
    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("index.html");
    let mut file = std::fs::File::create(&index_path).unwrap();
    write!(file, "<!DOCTYPE html><html><body>SPA</body></html>").unwrap();

    // Use create_spa_router which includes the SPA fallback
    let app = create_spa_router(ctx, temp_dir.path(), &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // SSE endpoint should return 200 with text/event-stream, NOT HTML
    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");

    assert!(
        content_type.starts_with("text/event-stream"),
        "SSE endpoint should return text/event-stream, not HTML. Got: {}",
        content_type
    );

    // Double-check: should NOT be HTML
    assert!(
        !content_type.contains("text/html"),
        "SSE endpoint was intercepted by SPA fallback (returned HTML)"
    );
}

#[tokio::test]
async fn nonexistent_route_returns_not_found() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn spa_fallback_returns_index_html() {
    use gglib_axum::routes::create_spa_router;
    use std::io::Write;
    use tempfile::TempDir;

    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Create a temp directory with an index.html
    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("index.html");
    let mut file = std::fs::File::create(&index_path).unwrap();
    write!(file, "<!DOCTYPE html><html><body>SPA</body></html>").unwrap();

    let app = create_spa_router(ctx, temp_dir.path(), &CorsConfig::AllowAll);

    // Request a non-existent client-side route (not under /api/)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/some/client/route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 200 with text/html from index.html
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or("").contains("text/html"))
            .unwrap_or(false)
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(body_str.contains("SPA"));
}

#[tokio::test]
async fn hf_search_endpoint_accepts_post_and_returns_valid_response() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    // Minimal valid request body for HF search
    let request_body = r#"{"query": "test", "page": 1}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(gglib_core::contracts::http::hf::SEARCH_PATH)
                .header("content-type", "application/json")
                .body(Body::from(request_body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should NOT return 404 (route not found) or 405 (method not allowed)
    // May return 200 (success) or 400/500 (validation/network error)
    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "HF search route should exist"
    );
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "HF search should accept POST method"
    );
}

// ============================================================================
// Settings API Tests (PR1 fixes - PUT alongside PATCH)
// ============================================================================

#[tokio::test]
async fn settings_endpoint_accepts_get() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return settings JSON
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn settings_endpoint_accepts_put() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    // Empty update request (no changes)
    let request_body = r#"{}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(request_body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should NOT return 405 Method Not Allowed
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "Settings should accept PUT method (frontend sends PUT)"
    );
}

#[tokio::test]
async fn settings_endpoint_accepts_patch() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    // Empty update request (no changes)
    let request_body = r#"{}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/settings")
                .header("content-type", "application/json")
                .body(Body::from(request_body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should NOT return 405 Method Not Allowed
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "Settings should also accept PATCH method (REST standard)"
    );
}

// ============================================================================
// Servers API Tests (PR1 fixes - collection routes with body)
// ============================================================================

#[tokio::test]
async fn servers_start_collection_route_accepts_post() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    // Request with model_id in body (matches frontend transport contract)
    let request_body = format!(r#"{{"model_id": 999, "port": {}}}"#, TEST_MODEL_PORT);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/servers/start")
                .header("content-type", "application/json")
                .body(Body::from(request_body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Contract validation: route accepts POST and returns JSON
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST should be allowed on /api/servers/start"
    );

    // Verify JSON response (content-type header)
    assert!(
        response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or("").starts_with("application/json"))
            .unwrap_or(false),
        "Should return application/json content-type"
    );
}

#[tokio::test]
async fn servers_stop_collection_route_accepts_post() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    // Request with model_id in body (matches frontend transport contract)
    let request_body = r#"{"model_id": 999}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/servers/stop")
                .header("content-type", "application/json")
                .body(Body::from(request_body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Contract validation: route accepts POST and returns JSON
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST should be allowed on /api/servers/stop"
    );

    // Verify JSON response (content-type header)
    assert!(
        response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or("").starts_with("application/json"))
            .unwrap_or(false),
        "Should return application/json content-type"
    );
}

// ============================================================================
// Proxy API Tests - real lifecycle integration
// ============================================================================

#[tokio::test]
async fn proxy_status_returns_stopped_when_not_running() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/proxy/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Contract: 200 OK with JSON
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or("").starts_with("application/json"))
            .unwrap_or(false),
        "Should return application/json content-type"
    );

    // Proxy not running initially, running == false
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body).unwrap();
    assert!(
        body_str.contains("\"running\":false"),
        "Proxy should report running:false when stopped, got: {}",
        body_str
    );
}

#[tokio::test]
async fn proxy_start_accepts_json_config() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let request_body = r#"null"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/proxy/start")
                .header("content-type", "application/json")
                .body(Body::from(request_body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Contract: returns status code (200 or error) with JSON response
    // Note: may fail if llama-server not installed, but should not panic
    assert!(
        response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or("").starts_with("application/json"))
            .unwrap_or(false),
        "Should return application/json content-type"
    );
}

#[tokio::test]
async fn proxy_stop_is_idempotent() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/proxy/stop")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Contract: idempotent - returns 200 even if already stopped
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or("").starts_with("application/json"))
            .unwrap_or(false),
        "Should return application/json content-type"
    );
}

// ============================================================================
// Downloads Queue GET Test (PR1 - already fixed locally)
// ============================================================================

#[tokio::test]
async fn downloads_queue_accepts_get() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/downloads/queue")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 200 with queue snapshot
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body_str = std::str::from_utf8(&body).unwrap();
    // Should contain queue snapshot fields
    assert!(body_str.contains("items"));
    assert!(body_str.contains("max_size"));
}

// ============================================================================
// Parameterized Route SPA-Guard Tests (PR3 - #249, #252)
// ============================================================================
// These tests ensure parameterized routes are properly matched and not
// intercepted by SPA fallback (which would return HTML instead of JSON).
// Regression guard for Axum 0.7 `:id` vs 0.8 `{id}` syntax mismatch.

#[tokio::test]
async fn model_get_by_id_returns_json_not_html() {
    use gglib_axum::routes::create_spa_router;
    use std::io::Write;
    use tempfile::TempDir;

    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Create a temp directory with an index.html (SPA fallback target)
    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("index.html");
    let mut file = std::fs::File::create(&index_path).unwrap();
    write!(file, "<!DOCTYPE html><html><body>SPA</body></html>").unwrap();

    let app = create_spa_router(ctx, temp_dir.path(), &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/models/test-model-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return JSON, not be intercepted by SPA fallback
    let content_type = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");

    assert!(
        !content_type.contains("text/html"),
        "Model GET /{{id}} was intercepted by SPA fallback (returned HTML). \
         Check route param syntax ('{{{{id}}}}' for Axum 0.8)"
    );
}

#[tokio::test]
async fn model_tags_by_id_returns_json_not_html() {
    use gglib_axum::routes::create_spa_router;
    use std::io::Write;
    use tempfile::TempDir;

    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("index.html");
    let mut file = std::fs::File::create(&index_path).unwrap();
    write!(file, "<!DOCTYPE html><html><body>SPA</body></html>").unwrap();

    let app = create_spa_router(ctx, temp_dir.path(), &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/models/test-model-id/tags")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let content_type = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");

    assert!(
        !content_type.contains("text/html"),
        "Model tags /{{id}}/tags was intercepted by SPA fallback (returned HTML). \
         Check route param syntax ('{{{{id}}}}' for Axum 0.8)"
    );
}

#[tokio::test]
async fn mcp_server_tools_by_id_returns_json_not_html() {
    use gglib_axum::routes::create_spa_router;
    use std::io::Write;
    use tempfile::TempDir;

    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("index.html");
    let mut file = std::fs::File::create(&index_path).unwrap();
    write!(file, "<!DOCTYPE html><html><body>SPA</body></html>").unwrap();

    let app = create_spa_router(ctx, temp_dir.path(), &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/mcp/servers/test-mcp-id/tools")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let content_type = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");

    assert!(
        !content_type.contains("text/html"),
        "MCP tools /{{id}}/tools was intercepted by SPA fallback (returned HTML). \
         Check route param syntax ('{{{{id}}}}' for Axum 0.8)"
    );
}

// ============================================================================
// Model Tags POST Body Test (PR4 - new bug found in UAT)
// ============================================================================

#[tokio::test]
async fn model_tags_accepts_post_with_body() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    // Frontend POSTs to /api/models/{id}/tags with { tag: "..." } in body
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/models/1/tags")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"tag":"test-tag"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should NOT return 405 Method Not Allowed
    assert_ne!(
        response.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /api/models/{{id}}/tags should be allowed (frontend sends tag in body)"
    );
}
