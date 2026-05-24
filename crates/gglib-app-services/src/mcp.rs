//! MCP server operations for GUI backend.

use std::sync::Arc;

use gglib_mcp::{
    McpEnvEntry, McpServerConfig, McpServerStatus, McpServerType, McpService, McpTool, NewMcpServer,
};

use crate::error::GuiError;
use crate::types::{
    CreateMcpServerRequest, McpEnvEntryDto, McpServerConfigDto, McpServerDto, McpServerInfo,
    McpServerStatusDto, McpToolCallRequest, McpToolCallResponse, McpToolInfo,
    UpdateMcpServerRequest,
};

/// Dependencies for MCP operations.
pub struct McpDeps {
    pub mcp: Arc<McpService>,
}

/// MCP server operations handler.
pub struct McpOps {
    mcp: Arc<McpService>,
}

impl McpOps {
    pub fn new(deps: McpDeps) -> Self {
        Self { mcp: deps.mcp }
    }

    /// Convert McpTool to McpToolInfo.
    fn tool_to_info(tool: &McpTool) -> McpToolInfo {
        McpToolInfo {
            name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
            title: tool.title.clone(),
        }
    }

    /// Convert core McpServer to DTO.
    fn server_to_dto(server: &gglib_core::McpServer) -> McpServerDto {
        McpServerDto {
            id: server.id,
            name: server.name.clone(),
            server_type: format!("{:?}", server.server_type).to_lowercase(),
            config: McpServerConfigDto {
                command: server.config.command.clone(),
                resolved_path_cache: server.config.resolved_path_cache.clone(),
                args: server.config.args.clone(),
                working_dir: server.config.working_dir.clone(),
                path_extra: server.config.path_extra.clone(),
                url: server.config.url.clone(),
            },
            enabled: server.enabled,
            lifecycle: server.lifecycle,
            env: server
                .env
                .iter()
                .map(|e| McpEnvEntryDto {
                    key: e.key.clone(),
                    value: e.value.clone(),
                })
                .collect(),
            created_at: server.created_at.to_rfc3339(),
            last_connected_at: server.last_connected_at.map(|dt| dt.to_rfc3339()),
            is_valid: server.is_valid,
            last_error: server.last_error.clone(),
        }
    }

    /// Convert status to DTO.
    fn status_to_dto(status: McpServerStatus) -> McpServerStatusDto {
        match status {
            McpServerStatus::Running => McpServerStatusDto::Running,
            McpServerStatus::Stopped => McpServerStatusDto::Stopped,
            McpServerStatus::Starting => McpServerStatusDto::Starting,
            McpServerStatus::Error(msg) => McpServerStatusDto::Error(msg),
        }
    }

    /// List all MCP server configurations with status.
    pub async fn list(&self) -> Result<Vec<McpServerInfo>, GuiError> {
        let servers = self
            .mcp
            .list_servers_with_status()
            .await
            .map_err(GuiError::from)?;

        Ok(servers
            .into_iter()
            .map(|info| McpServerInfo {
                server: Self::server_to_dto(&info.server),
                status: Self::status_to_dto(info.status),
                tools: info.tools.iter().map(Self::tool_to_info).collect(),
            })
            .collect())
    }

    /// Add a new MCP server configuration.
    pub async fn add(&self, req: CreateMcpServerRequest) -> Result<McpServerInfo, GuiError> {
        let server_type = match req.server_type.as_str() {
            "stdio" => McpServerType::Stdio,
            "sse" => McpServerType::Sse,
            _ => {
                return Err(GuiError::ValidationFailed(format!(
                    "Invalid server type: {}",
                    req.server_type
                )));
            }
        };

        let config = McpServerConfig {
            command: req.command,
            resolved_path_cache: None, // Will be populated by resolver
            args: Some(req.args),
            working_dir: req.working_dir,
            path_extra: req.path_extra,
            url: req.url,
        };

        let new_server = NewMcpServer {
            name: req.name,
            server_type,
            config,
            env: req
                .env
                .into_iter()
                .map(|e| McpEnvEntry {
                    key: e.key,
                    value: e.value,
                })
                .collect(),
            enabled: true,
            lifecycle: req.lifecycle,
        };

        let server = self
            .mcp
            .add_server(new_server)
            .await
            .map_err(GuiError::from)?;

        Ok(McpServerInfo {
            server: Self::server_to_dto(&server),
            status: McpServerStatusDto::Stopped,
            tools: Vec::new(),
        })
    }

    /// Update an MCP server configuration.
    pub async fn update(
        &self,
        id: i64,
        req: UpdateMcpServerRequest,
    ) -> Result<McpServerInfo, GuiError> {
        let mut server = self
            .mcp
            .get_server(id)
            .await
            .map_err(|e| GuiError::Internal(e.to_string()))?;

        if let Some(name) = req.name {
            server.name = name;
        }
        if let Some(command) = req.command {
            server.config.command = Some(command);
            // Clear cache when command changes
            server.config.resolved_path_cache = None;
        }
        if let Some(args) = req.args {
            server.config.args = Some(args);
        }
        if let Some(working_dir) = req.working_dir {
            server.config.working_dir = Some(working_dir);
        }
        if let Some(path_extra) = req.path_extra {
            server.config.path_extra = Some(path_extra);
        }
        if let Some(url) = req.url {
            server.config.url = Some(url);
        }
        if let Some(env) = req.env {
            server.env = env
                .into_iter()
                .map(|e| McpEnvEntry {
                    key: e.key,
                    value: e.value,
                })
                .collect();
        }
        if let Some(enabled) = req.enabled {
            server.enabled = enabled;
        }
        if let Some(lifecycle) = req.lifecycle {
            server.lifecycle = lifecycle;
        }

        self.mcp
            .update_server(server.clone())
            .await
            .map_err(GuiError::from)?;

        let status = self.mcp.get_server_status(id).await;

        Ok(McpServerInfo {
            server: Self::server_to_dto(&server),
            status: Self::status_to_dto(status),
            tools: Vec::new(),
        })
    }

    /// Remove an MCP server configuration.
    pub async fn remove(&self, id: i64) -> Result<(), GuiError> {
        self.mcp.remove_server(id).await.map_err(GuiError::from)
    }

    /// Start an MCP server.
    pub async fn start(&self, id: i64) -> Result<McpServerInfo, GuiError> {
        let tools = self.mcp.start_server(id).await.map_err(GuiError::from)?;
        let info = self.mcp.get_server_info(id).await.map_err(GuiError::from)?;

        Ok(McpServerInfo {
            server: Self::server_to_dto(&info.server),
            status: Self::status_to_dto(info.status),
            tools: tools.iter().map(Self::tool_to_info).collect(),
        })
    }

    /// Stop an MCP server.
    pub async fn stop(&self, id: i64) -> Result<McpServerInfo, GuiError> {
        self.mcp.stop_server(id).await.map_err(GuiError::from)?;
        let info = self.mcp.get_server_info(id).await.map_err(GuiError::from)?;

        Ok(McpServerInfo {
            server: Self::server_to_dto(&info.server),
            status: Self::status_to_dto(info.status),
            tools: Vec::new(),
        })
    }

    /// List available tools from a running MCP server.
    pub async fn list_tools(&self, id: i64) -> Result<Vec<McpToolInfo>, GuiError> {
        let tools = self
            .mcp
            .list_server_tools(id)
            .await
            .map_err(|e| GuiError::Internal(e.to_string()))?;
        Ok(tools.iter().map(Self::tool_to_info).collect())
    }

    /// Call a tool on a running MCP server.
    pub async fn call_tool(
        &self,
        id: i64,
        req: McpToolCallRequest,
    ) -> Result<McpToolCallResponse, GuiError> {
        let result = self
            .mcp
            .call_tool(id, &req.tool_name, req.arguments)
            .await
            .map_err(|e| GuiError::Internal(e.to_string()))?;

        // McpToolResult has: success, data, error
        Ok(McpToolCallResponse {
            success: result.success,
            data: result.data,
            error: result.error,
        })
    }

    /// Resolve MCP server executable path (thin wrapper for diagnostics/auto-fix).
    pub async fn resolve_path(
        &self,
        id: i64,
    ) -> Result<gglib_core::ports::ResolutionStatus, GuiError> {
        self.mcp
            .ensure_resolved(id)
            .await
            .map_err(|e| GuiError::Internal(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gglib_core::ports::NoopEmitter;
    use gglib_core::McpLifecycle;
    use gglib_db::CoreFactory;

    use super::*;
    use gglib_db::setup_test_database;

    async fn make_ops() -> McpOps {
        let pool = setup_test_database().await.expect("in-memory DB");
        let repo = CoreFactory::mcp_repository(pool);
        let mcp = Arc::new(McpService::new(repo, Arc::new(NoopEmitter::new())));
        McpOps::new(McpDeps { mcp })
    }

    fn stdio_req(name: &str) -> CreateMcpServerRequest {
        CreateMcpServerRequest {
            name: name.to_string(),
            server_type: "stdio".to_string(),
            command: Some("echo".to_string()),
            args: vec![],
            working_dir: None,
            path_extra: None,
            url: None,
            env: vec![],
            lifecycle: McpLifecycle::Lazy,
        }
    }

    #[tokio::test]
    async fn list_returns_empty_on_fresh_db() {
        let ops = make_ops().await;
        let servers = ops.list().await.expect("list should succeed");
        assert!(servers.is_empty());
    }

    #[tokio::test]
    async fn add_server_appears_in_list() {
        let ops = make_ops().await;
        ops.add(stdio_req("test-server"))
            .await
            .expect("add should succeed");

        let servers = ops.list().await.unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].server.name, "test-server");
    }

    #[tokio::test]
    async fn invalid_server_type_returns_validation_error() {
        let ops = make_ops().await;
        let mut req = stdio_req("bad");
        req.server_type = "grpc".to_string(); // unsupported
        let result = ops.add(req).await;
        assert!(
            matches!(result, Err(GuiError::ValidationFailed(_))),
            "expected ValidationFailed, got {result:?}"
        );
    }

    #[tokio::test]
    async fn remove_server_deletes_it() {
        let ops = make_ops().await;
        let info = ops.add(stdio_req("to-delete")).await.unwrap();
        ops.remove(info.server.id)
            .await
            .expect("remove should succeed");
        let servers = ops.list().await.unwrap();
        assert!(servers.is_empty());
    }
}
