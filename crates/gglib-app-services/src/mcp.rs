//! MCP server operations for GUI backend.

use std::sync::Arc;

use gglib_core::McpServiceError;
use gglib_mcp::{
    McpEnvEntry, McpServerConfig, McpServerStatus, McpServerType, McpService, McpTool, NewMcpServer,
};

use crate::error::GuiError;
use crate::types::{
    CreateMcpServerRequest, McpEnvEntryDto, McpServerConfigDto, McpServerDto, McpServerInfo,
    McpServerStatusDto, McpTestResult, McpToolInfo, UpdateMcpServerRequest,
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

    /// Convert `McpTool` to `McpToolInfo`.
    fn tool_to_info(tool: &McpTool) -> McpToolInfo {
        McpToolInfo {
            name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
            title: tool.title.clone(),
        }
    }

    /// Convert core `McpServer` to DTO.
    fn server_to_dto(server: &gglib_core::McpServer) -> McpServerDto {
        McpServerDto {
            id: server.id,
            name: server.name.clone(),
            server_type: server.server_type.to_string(),
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
        let server_type = req
            .server_type
            .parse::<McpServerType>()
            .map_err(GuiError::ValidationFailed)?;

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

    /// Test a configured server end to end — `gglib mcp test`.
    ///
    /// Starts a throwaway instance of the stored configuration, lists what it
    /// offers, then stops it. Distinct from the `tools` that [`Self::list`]
    /// reports, which are filled in only for a server that is already running
    /// and so say nothing about whether it *can* start. This is the answer to
    /// "is this config right?", and without it the only way to find out is a
    /// chat that silently has no tools.
    pub async fn test_connection(&self, id: i64) -> Result<McpTestResult, GuiError> {
        // A failed connection is the expected outcome of a misconfiguration,
        // not a fault — report it as a result the UI can render beside the
        // config rather than an error that replaces the panel. Only a server
        // that could not be read at all is an error.
        match self.mcp.test_server(id).await {
            Ok(tools) => Ok(McpTestResult {
                ok: true,
                error: None,
                tools: tools.iter().map(Self::tool_to_info).collect(),
            }),
            Err(McpServiceError::Repository(_)) => Err(GuiError::NotFound {
                entity: "MCP server",
                id: id.to_string(),
            }),
            Err(e) => Ok(McpTestResult {
                ok: false,
                error: Some(e.to_string()),
                tools: Vec::new(),
            }),
        }
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
#[path = "mcp_tests.rs"]
mod tests;
