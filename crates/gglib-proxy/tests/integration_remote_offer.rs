//! The proxy's drawing answer names the MCP servers a paired device's run is
//! offered, `mcp_servers`, beside whether it can draw: what the live session
//! opens, and `[]` when it opens none or there is no tunnel.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::RemoteGatewayPort;
use gglib_core::{
    CorsConfig, McpRepositoryError, McpServer, McpServerRepository, NewMcpServer, ProxyAccessConfig,
};
use gglib_mcp::McpService;
use gglib_proxy::ServeConfig;
use reqwest::StatusCode;
use serde_json::{Value, json};

mod fixtures;
use fixtures::remote::StubGateway;

/// A repository holding one valid, enabled server named `shell`, which the
/// tunnel never opens in these tests.
struct OneServer(McpServer);

impl OneServer {
    fn new() -> Self {
        let new = NewMcpServer::new_stdio("shell", "sh", vec![], None);
        let server = serde_json::from_value(json!({
            "id": 1,
            "name": new.name,
            "server_type": new.server_type,
            "config": new.config,
            "enabled": true,
            "lifecycle": new.lifecycle,
            "env": [],
            "created_at": "2026-10-23T00:00:00Z",
            "is_valid": true,
        }))
        .unwrap();
        Self(server)
    }
}

#[async_trait]
impl McpServerRepository for OneServer {
    async fn insert(&self, _new: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::Internal("read only".into()))
    }

    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        (id == self.0.id)
            .then(|| self.0.clone())
            .ok_or_else(|| McpRepositoryError::NotFound(id.to_string()))
    }

    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        (name == self.0.name)
            .then(|| self.0.clone())
            .ok_or_else(|| McpRepositoryError::NotFound(name.into()))
    }

    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        Ok(vec![self.0.clone()])
    }

    async fn update(&self, _server: &McpServer) -> Result<(), McpRepositoryError> {
        Ok(())
    }

    async fn delete(&self, _id: i64) -> Result<(), McpRepositoryError> {
        Ok(())
    }

    async fn update_last_connected(&self, _id: i64) -> Result<(), McpRepositoryError> {
        Ok(())
    }
}

const TOKEN: &str = "secret123";

/// The drawing answer of a proxy with no image driver, one MCP server
/// configured, and `gateway` as its tunnel owner, if any.
async fn drawing(gateway: Option<StubGateway>) -> Value {
    let remote = gateway.map(|gateway| Arc::new(gateway) as Arc<dyn RemoteGatewayPort>);
    let access = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        Some(TOKEN.to_owned()),
        "127.0.0.1",
        vec![],
    )
    .with_remote(remote);
    let proxy = fixtures::spawn::spawn(ServeConfig {
        access,
        mcp: Arc::new(McpService::new(Arc::new(OneServer::new()))),
        ..fixtures::spawn::defaults().await
    })
    .await;
    let (base, cancel) = (proxy.base, proxy.cancel);
    let response = reqwest::Client::new()
        .get(format!("{base}/v1/images/drawing"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let answer = response.json().await.unwrap();
    cancel.cancel();
    answer
}

/// The servers the session opens are named, also when it cannot draw, and
/// the drawing answer's own fields are still there beside them.
#[tokio::test]
async fn the_drawing_answer_names_the_servers_the_tunnel_opens() {
    let answer = drawing(Some(StubGateway::opening(&["tavily", "fetch"]))).await;
    assert_eq!(answer["mcp_servers"], json!(["tavily", "fetch"]));
    assert_eq!(answer["available"], false);
    assert_eq!(answer["code"], "drawing_unavailable");
    assert_eq!(answer["models"], json!([]));
}

/// None open, or no tunnel owner at all: `[]`, never absent, and never the
/// servers merely configured here.
#[tokio::test]
async fn the_drawing_answer_names_none_while_none_are_open() {
    for gateway in [Some(StubGateway::new()), None] {
        let answer = drawing(gateway).await;
        assert_eq!(answer["mcp_servers"], json!([]), "{answer}");
    }
}
