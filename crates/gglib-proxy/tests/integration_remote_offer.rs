//! The proxy's drawing answer lists the MCP servers a paired device's run may
//! use, `mcp_servers`, each by id and name, beside whether it can draw: the
//! servers open to paired devices while a session is live, and `[]` when
//! none is open, no session is live, or there is no tunnel.

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

/// A repository holding valid, enabled stdio servers, each open to paired
/// devices or not, listed in the order given.
struct Servers(Vec<McpServer>);

/// A valid, enabled stdio server `id` named `name`, open to paired devices
/// when `open` says.
fn server(id: i64, name: &str, open: bool) -> McpServer {
    let new = NewMcpServer::new_stdio(name, "sh", vec![], None);
    serde_json::from_value(json!({
        "id": id,
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
    .unwrap()
}

impl Servers {
    /// One server, `shell`, open to paired devices or not.
    fn one(open: bool) -> Self {
        Self(vec![server(1, "shell", open)])
    }
}

#[async_trait]
impl McpServerRepository for Servers {
    async fn insert(&self, _new: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::Internal("read only".into()))
    }

    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        self.0
            .iter()
            .find(|server| server.id == id)
            .cloned()
            .ok_or_else(|| McpRepositoryError::NotFound(id.to_string()))
    }

    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        self.0
            .iter()
            .find(|server| server.name == name)
            .cloned()
            .ok_or_else(|| McpRepositoryError::NotFound(name.into()))
    }

    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        Ok(self.0.clone())
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

/// The drawing answer of a proxy with no image driver, `servers` configured,
/// and `gateway` as its tunnel owner, if any.
async fn drawing(gateway: Option<StubGateway>, servers: Servers) -> Value {
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
        mcp: Arc::new(McpService::new(Arc::new(servers))),
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

/// A server open to paired devices is listed by its id and name while a
/// session is live, also when it cannot draw, and the drawing answer's own
/// fields are still there beside it.
#[tokio::test]
async fn the_drawing_answer_names_the_open_servers_while_a_session_is_live() {
    let servers = Servers::one(true);
    let id = servers.0[0].id;
    let answer = drawing(Some(StubGateway::serving()), servers).await;
    assert_eq!(
        answer["mcp_servers"],
        json!([{ "id": id, "name": "shell" }])
    );
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
        let answer = drawing(gateway, Servers::one(open)).await;
        assert_eq!(answer["mcp_servers"], json!([]), "{answer}");
    }
}

/// Two open servers sharing a name are each listed, with their own ids, in
/// order of name and then id, whatever order they are stored in; a closed
/// one is not listed.
#[tokio::test]
async fn two_open_servers_of_one_name_are_each_listed_with_their_id() {
    let servers = Servers(vec![
        server(9, "fs", true),
        server(3, "tavily", true),
        server(5, "closed", false),
        server(4, "fs", true),
    ]);
    let answer = drawing(Some(StubGateway::serving()), servers).await;
    assert_eq!(
        answer["mcp_servers"],
        json!([
            { "id": 4, "name": "fs" },
            { "id": 9, "name": "fs" },
            { "id": 3, "name": "tavily" },
        ])
    );
}
