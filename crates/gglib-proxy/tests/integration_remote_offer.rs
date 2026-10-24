//! The proxy's drawing answer names the MCP servers a paired device's run is
//! offered, `mcp_servers`, beside whether it can draw: the servers open to
//! paired devices while a session is live, and `[]` when none is open, no
//! session is live, or there is no tunnel.

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

/// A repository holding one valid, enabled stdio server named `shell`, open
/// to paired devices or not.
struct OneServer(McpServer);

impl OneServer {
    fn new(open: bool) -> Self {
        let new = NewMcpServer::new_stdio("shell", "sh", vec![], None);
        let server = serde_json::from_value(json!({
            "id": 1,
            "name": new.name,
            "server_type": new.server_type,
            "config": new.config,
            "enabled": true,
            "allow_remote": open,
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

    async fn set_allow_remote(&self, _id: i64, _allow: bool) -> Result<(), McpRepositoryError> {
        Err(McpRepositoryError::Internal("read only".into()))
    }
}

const TOKEN: &str = "secret123";

/// The drawing answer of a proxy with no image driver, one MCP server
/// configured, open to paired devices when `open` says, and `gateway` as its
/// tunnel owner, if any.
async fn drawing(gateway: Option<StubGateway>, open: bool) -> Value {
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
        mcp: Arc::new(McpService::new(Arc::new(OneServer::new(open)))),
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

/// A server open to paired devices is named while a session is live, also
/// when it cannot draw, and the drawing answer's own fields are still there
/// beside it.
#[tokio::test]
async fn the_drawing_answer_names_the_open_servers_while_a_session_is_live() {
    let answer = drawing(Some(StubGateway::serving()), true).await;
    assert_eq!(answer["mcp_servers"], json!(["shell"]));
    assert_eq!(answer["available"], false);
    assert_eq!(answer["code"], "drawing_unavailable");
    assert_eq!(answer["models"], json!([]));
}

/// An open server with no live session, or no tunnel owner at all, and a
/// server merely configured here under a live session: `[]`, never absent.
#[tokio::test]
async fn the_drawing_answer_names_none_without_a_session_or_an_open_server() {
    for (gateway, open) in [
        (Some(StubGateway::new()), true),
        (None, true),
        (Some(StubGateway::serving()), false),
    ] {
        let answer = drawing(gateway, open).await;
        assert_eq!(answer["mcp_servers"], json!([]), "{answer}");
    }
}
