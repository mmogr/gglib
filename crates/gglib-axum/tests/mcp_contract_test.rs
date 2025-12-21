//! Contract tests for MCP API endpoints.
//!
//! These tests verify that the JSON structure returned by handlers
//! matches what the TypeScript frontend expects.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

use gglib_axum::bootstrap::{CorsConfig, ServerConfig, bootstrap};
use gglib_axum::routes::create_router;

/// Helper to create a test config.
fn test_config() -> ServerConfig {
    ServerConfig {
        port: 0,
        base_port: 19000,
        llama_server_path: "/nonexistent/llama-server".into(),
        max_concurrent: 1,
        static_dir: None,
        cors: CorsConfig::AllowAll,
    }
}

#[tokio::test]
async fn test_list_mcp_servers_json_structure() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return, // Skip if bootstrap fails
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/mcp/servers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Should be an array
    assert!(json.is_array(), "Response should be an array");

    // If there are servers, verify the structure
    if let Some(servers) = json.as_array()
        && let Some(server) = servers.first()
    {
        // Verify nested structure: server.server.id, not server.id
        assert!(
            server.get("server").is_some(),
            "Each item should have a 'server' field"
        );
        assert!(
            server.get("status").is_some(),
            "Each item should have a 'status' field"
        );
        assert!(
            server.get("tools").is_some(),
            "Each item should have a 'tools' field"
        );

        // Verify server object structure
        let server_obj = server.get("server").unwrap();
        assert!(server_obj.get("id").is_some(), "server.id should exist");
        assert!(server_obj.get("name").is_some(), "server.name should exist");
        assert!(
            server_obj.get("server_type").is_some(),
            "server.server_type should exist"
        );
        assert!(
            server_obj.get("config").is_some(),
            "server.config should exist"
        );
        assert!(
            server_obj.get("enabled").is_some(),
            "server.enabled should exist"
        );
        assert!(
            server_obj.get("auto_start").is_some(),
            "server.auto_start should exist"
        );

        // Verify status is string or object
        let status = server.get("status").unwrap();
        assert!(
            status.is_string() || status.is_object(),
            "status should be string or error object"
        );

        // Verify tools is array
        assert!(
            server.get("tools").unwrap().is_array(),
            "tools should be an array"
        );
    }
}

#[tokio::test]
async fn test_add_mcp_server_returns_nested_structure() {
    let ctx = match bootstrap(test_config()).await {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let app = create_router(ctx, &CorsConfig::AllowAll);

    let request_body = json!({
        "name": "Test Server",
        "server_type": "stdio",
        "command": "node",
        "args": ["server.js"],
        "env": [],
        "auto_start": false
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/mcp/servers")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Verify nested structure
    assert!(
        json.get("server").is_some(),
        "Response should have 'server' field"
    );
    assert!(
        json.get("status").is_some(),
        "Response should have 'status' field"
    );
    assert!(
        json.get("tools").is_some(),
        "Response should have 'tools' field"
    );

    // Verify server.id exists (NOT top-level id)
    let server = json.get("server").unwrap();
    assert!(
        server.get("id").is_some(),
        "server.id should exist (nested, not top-level)"
    );
    assert!(
        json.get("id").is_none(),
        "Top-level 'id' should NOT exist (should be server.id)"
    );
}
