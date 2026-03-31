//! Integration tests for the MCP Streamable HTTP gateway.
//!
//! Spins up the proxy with mocked ports and exercises the full
//! `POST /mcp`, `GET /mcp`, and `DELETE /mcp` protocol flow.

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use gglib_core::ports::{
    CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError, ModelRuntimePort,
    ModelSummary, RunningTarget,
};
use gglib_core::{McpRepositoryError, McpServer, McpServerRepository, NewMcpServer, NoopEmitter};
use gglib_mcp::McpService;

// ─── Mock ports ────────────────────────────────────────────────────────────

#[derive(Debug)]
struct MockRuntimePort;

#[async_trait]
impl ModelRuntimePort for MockRuntimePort {
    async fn ensure_model_running(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        Ok(RunningTarget::local(9999, 1, "test-model".into(), 4096))
    }
    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

#[derive(Debug)]
struct MockCatalogPort;

#[async_trait]
impl ModelCatalogPort for MockCatalogPort {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(vec![])
    }
    async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(None)
    }
    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// Minimal in-memory MCP repository that always returns an empty server list.
struct EmptyMcpRepo;

#[async_trait]
impl McpServerRepository for EmptyMcpRepo {
    async fn insert(&self, _s: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::Internal("not implemented".into()))
    }
    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(id.to_string()))
    }
    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(name.into()))
    }
    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        Ok(vec![])
    }
    async fn update(&self, _s: &McpServer) -> Result<(), McpRepositoryError> {
        Ok(())
    }
    async fn delete(&self, _id: i64) -> Result<(), McpRepositoryError> {
        Ok(())
    }
    async fn update_last_connected(&self, _id: i64) -> Result<(), McpRepositoryError> {
        Ok(())
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Start the proxy on a random port and return (base_url, cancel_token).
async fn start_proxy() -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(MockRuntimePort);
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(MockCatalogPort);
    let mcp = Arc::new(McpService::new(
        Arc::new(EmptyMcpRepo),
        Arc::new(NoopEmitter::new()),
    ));

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        gglib_proxy::serve(listener, 4096, runtime, catalog, mcp, cancel_clone)
            .await
            .ok();
    });

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (format!("http://{addr}"), cancel)
}

/// Send a JSON-RPC request to POST /mcp.
async fn post_mcp(
    client: &Client,
    base_url: &str,
    body: Value,
    session_id: Option<&str>,
) -> reqwest::Response {
    let mut req = client.post(format!("{base_url}/mcp")).json(&body);
    if let Some(sid) = session_id {
        req = req.header("mcp-session-id", sid);
    }
    req.send().await.unwrap()
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_mcp_returns_405() {
    let (base_url, cancel) = start_proxy().await;
    let client = Client::new();

    let resp = client.get(format!("{base_url}/mcp")).send().await.unwrap();

    assert_eq!(resp.status(), 405);

    cancel.cancel();
}

#[tokio::test]
async fn post_mcp_invalid_json_returns_parse_error() {
    let (base_url, cancel) = start_proxy().await;
    let client = Client::new();

    let resp = client
        .post(format!("{base_url}/mcp"))
        .header("content-type", "application/json")
        .body("not json")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32700); // PARSE_ERROR

    cancel.cancel();
}

#[tokio::test]
async fn post_mcp_unknown_method_returns_method_not_found() {
    let (base_url, cancel) = start_proxy().await;
    let client = Client::new();

    // First initialize to get a session
    let init_resp = post_mcp(
        &client,
        &base_url,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"}
            }
        }),
        None,
    )
    .await;
    let session_id = init_resp
        .headers()
        .get("mcp-session-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // Send unknown method
    let resp = post_mcp(
        &client,
        &base_url,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "nonexistent/method"
        }),
        Some(&session_id),
    )
    .await;

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32601); // METHOD_NOT_FOUND

    cancel.cancel();
}

#[tokio::test]
async fn full_happy_path_initialize_list_delete() {
    let (base_url, cancel) = start_proxy().await;
    let client = Client::new();

    // ── Step 1: Initialize ──
    let resp = post_mcp(
        &client,
        &base_url,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "integration-test", "version": "0.1"}
            }
        }),
        None,
    )
    .await;

    assert_eq!(resp.status(), 200);
    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .expect("initialize must return Mcp-Session-Id header")
        .to_str()
        .unwrap()
        .to_string();

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 1);
    assert_eq!(body["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(body["result"]["serverInfo"]["name"], "gglib");
    assert!(body["result"]["capabilities"]["tools"].is_object());

    // ── Step 2: Send notifications/initialized ──
    let resp = post_mcp(
        &client,
        &base_url,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
        Some(&session_id),
    )
    .await;

    assert_eq!(resp.status(), 202);

    // ── Step 3: tools/list ──
    let resp = post_mcp(
        &client,
        &base_url,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        }),
        Some(&session_id),
    )
    .await;

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 2);
    // Empty repo → empty tools list
    let tools = body["result"]["tools"].as_array().unwrap();
    assert!(tools.is_empty());

    // ── Step 4: ping ──
    let resp = post_mcp(
        &client,
        &base_url,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "ping"
        }),
        Some(&session_id),
    )
    .await;

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["id"], 3);
    assert!(body["result"].is_object());

    // ── Step 5: DELETE /mcp — terminate session ──
    let resp = client
        .delete(format!("{base_url}/mcp"))
        .header("mcp-session-id", &session_id)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    // ── Step 6: Verify session is gone — tools/list should fail ──
    let resp = post_mcp(
        &client,
        &base_url,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/list"
        }),
        Some(&session_id),
    )
    .await;

    // Session gone → 404 with "Unknown or expired session"
    assert_eq!(resp.status(), 404);

    cancel.cancel();
}

#[tokio::test]
async fn missing_session_id_on_non_initialize_returns_400() {
    let (base_url, cancel) = start_proxy().await;
    let client = Client::new();

    let resp = post_mcp(
        &client,
        &base_url,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        }),
        None, // no session ID
    )
    .await;

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32600); // INVALID_REQUEST

    cancel.cancel();
}

#[tokio::test]
async fn invalid_session_id_returns_404() {
    let (base_url, cancel) = start_proxy().await;
    let client = Client::new();

    let resp = post_mcp(
        &client,
        &base_url,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        }),
        Some("nonexistent-session-id"),
    )
    .await;

    assert_eq!(resp.status(), 404);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32600);

    cancel.cancel();
}

#[tokio::test]
async fn delete_mcp_without_session_header_returns_400() {
    let (base_url, cancel) = start_proxy().await;
    let client = Client::new();

    let resp = client
        .delete(format!("{base_url}/mcp"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);

    cancel.cancel();
}

#[tokio::test]
async fn delete_mcp_with_unknown_session_returns_404() {
    let (base_url, cancel) = start_proxy().await;
    let client = Client::new();

    let resp = client
        .delete(format!("{base_url}/mcp"))
        .header("mcp-session-id", "does-not-exist")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);

    cancel.cancel();
}

#[tokio::test]
async fn disallowed_origin_returns_403() {
    let (base_url, cancel) = start_proxy().await;
    let client = Client::new();

    let resp = client
        .post(format!("{base_url}/mcp"))
        .header("origin", "https://evil.example.com")
        .header("content-type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "test"}
            }
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 403);

    cancel.cancel();
}

#[tokio::test]
async fn localhost_origin_is_allowed() {
    let (base_url, cancel) = start_proxy().await;
    let client = Client::new();

    let resp = client
        .post(format!("{base_url}/mcp"))
        .header("origin", "http://localhost:3000")
        .header("content-type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "test"}
            }
        }))
        .send()
        .await
        .unwrap();

    // Should succeed (not 403)
    assert_eq!(resp.status(), 200);

    cancel.cancel();
}

#[tokio::test]
async fn tools_call_unknown_tool_returns_error() {
    let (base_url, cancel) = start_proxy().await;
    let client = Client::new();

    // Initialize first
    let resp = post_mcp(
        &client,
        &base_url,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {"name": "test"}
            }
        }),
        None,
    )
    .await;
    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // Call a tool that doesn't exist — tools/call returns SSE so we read text
    let resp = client
        .post(format!("{base_url}/mcp"))
        .header("mcp-session-id", &session_id)
        .header("content-type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "nonexistent__tool",
                "arguments": {}
            }
        }))
        .send()
        .await
        .unwrap();

    // tools/call returns 200 (error is in the JSON-RPC body, not HTTP status)
    assert_eq!(resp.status(), 200);

    // For tools/call the response is SSE; parse the event data
    let text = resp.text().await.unwrap();
    assert!(text.contains("Unknown tool"));

    cancel.cancel();
}
