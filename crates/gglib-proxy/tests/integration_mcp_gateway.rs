//! Integration tests for the MCP Streamable HTTP gateway.
//!
//! Spins up the proxy with mocked ports and exercises the full
//! `POST /mcp`, `GET /mcp`, and `DELETE /mcp` protocol flow.

use std::sync::Arc;

use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};

mod fixtures;
use fixtures::common::{EmptyCatalog, MockSettingsRepo, NoopRuntime, make_mcp_service};

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Start the proxy on a random port and return (base_url, cancel_token).
async fn start_proxy() -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(NoopRuntime);
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(EmptyCatalog);

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            Some(4096),
            // Device memory readable: this suite is not about the fit.
            true,
            runtime,
            catalog,
            make_mcp_service(),
            cancel_clone,
            Arc::new(MockSettingsRepo),
            None, // inference_override
            None, // default_profile
            false,
            None,
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            std::sync::Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
            std::sync::Arc::new(gglib_core::domain::defects::ModelDefectLedger::new()),
            &gglib_core::ProxyAccessConfig::default(),
        )
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
    // Progressive disclosure: always exactly 3 meta-tools regardless of how
    // many MCP servers are running.
    let tools = body["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 3);
    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tool_names.contains(&"search_tools"));
    assert!(tool_names.contains(&"get_tool_schema"));
    assert!(tool_names.contains(&"invoke_tool"));

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
