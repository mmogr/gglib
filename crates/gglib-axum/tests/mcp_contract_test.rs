//! Contract tests for MCP API endpoints.
//!
//! These tests verify that the JSON structure returned by handlers
//! matches what the TypeScript frontend expects.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use common::harness::test_app;
use gglib_core::CorsConfig;

/// The server the list test seeds and then looks for by name.
const SEEDED_NAME: &str = "List Contract Server";

/// Register a server and return the decoded response body.
///
/// Both tests need a server that exists: the list route has nothing to
/// describe until something is registered, and an empty list is exactly the
/// state a clean machine starts in.
async fn add_server(app: &Router, name: &str) -> Value {
    let body = json!({
        "name": name,
        "server_type": "stdio",
        "command": "node",
        "args": ["server.js"],
        "env": [],
        "lifecycle": "lazy"
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/mcp/servers")
                .header("Host", "127.0.0.1:9887")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK, "POST /api/mcp/servers");

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Assert the `{ server, status, tools }` envelope the frontend destructures.
fn assert_server_envelope(entry: &Value) {
    // `is_some()` alone is not enough: `Value::get` returns `Some(Null)` for a
    // field serialised as `null`, and the frontend destructures these.
    for field in ["server", "status", "tools"] {
        assert!(
            entry.get(field).is_some_and(|value| !value.is_null()),
            "entry should have a non-null '{field}' field, got {entry}"
        );
    }

    let server = entry.get("server").unwrap();
    for field in [
        "id",
        "name",
        "server_type",
        "config",
        "enabled",
        "lifecycle",
    ] {
        assert!(
            server.get(field).is_some_and(|value| !value.is_null()),
            "server.{field} should exist and be non-null, got {server}"
        );
    }

    let status = entry.get("status").unwrap();
    assert!(
        status.is_string() || status.is_object(),
        "status should be string or error object, got {status}"
    );
    assert!(
        entry.get("tools").unwrap().is_array(),
        "tools should be an array"
    );
}

#[tokio::test]
async fn test_list_mcp_servers_json_structure() {
    let app = test_app(CorsConfig::AllowAll).await;

    // Seed one, so the structural assertions below always have a subject.
    // They used to sit behind `if let Some(server) = servers.first()`, which
    // on a clean checkout skipped every one of them.
    add_server(&app, SEEDED_NAME).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/mcp/servers")
                .header("Host", "127.0.0.1:9887")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let servers = json.as_array().expect("response should be an array");

    // Found by name rather than by index, so this stays a shape assertion. A
    // bare `len() == 1` would also pin "the product ships no builtin MCP
    // servers", which is a product decision, not a wire contract.
    let seeded = servers
        .iter()
        .find(|entry| entry.pointer("/server/name") == Some(&json!(SEEDED_NAME)))
        .unwrap_or_else(|| panic!("seeded server missing from the list, got {json}"));

    // Nested under `server`, not flattened onto the entry.
    assert_server_envelope(seeded);
}

#[tokio::test]
async fn test_add_mcp_server_returns_nested_structure() {
    let app = test_app(CorsConfig::AllowAll).await;

    let json = add_server(&app, "Test Server").await;

    assert_server_envelope(&json);
    assert!(
        json.get("id").is_none(),
        "top-level 'id' should NOT exist (should be server.id), got {json}"
    );
}
