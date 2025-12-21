//! MCP server lifecycle management.
//!
//! Manages starting, stopping, and monitoring MCP server processes.
//! This module depends only on core traits and the MCP client - no direct
//! process spawning outside of the client.

use crate::client::{McpClient, McpClientError};
use gglib_core::{McpServer, McpServerStatus, McpServerType, McpTool, McpToolResult};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Errors that can occur during MCP manager operations.
#[derive(Debug, Error)]
pub enum McpManagerError {
    #[error("Server already running: {0}")]
    AlreadyRunning(String),

    #[error("Server not running: {0}")]
    NotRunning(String),

    #[error("Failed to start server: {0}")]
    StartFailed(String),

    #[error("Client error: {0}")]
    ClientError(#[from] McpClientError),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Running MCP server instance.
struct RunningServer {
    /// Server configuration (kept for debugging/future use)
    _server: McpServer,
    /// MCP client for communication
    client: McpClient,
    /// Current status
    status: McpServerStatus,
    /// Discovered tools
    tools: Vec<McpTool>,
}

/// Manager for MCP server lifecycle.
///
/// This manager handles starting, stopping, and querying MCP server processes.
/// It uses the `McpClient` for protocol communication but does not directly
/// interact with the database - that's the service's responsibility.
pub struct McpManager {
    /// Running servers indexed by server ID
    servers: Arc<RwLock<HashMap<i64, RunningServer>>>,
}

impl McpManager {
    /// Create a new MCP manager.
    pub fn new() -> Self {
        Self {
            servers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start an MCP server.
    ///
    /// For stdio servers, spawns the process and initializes the MCP session.
    /// For SSE servers, establishes the HTTP connection.
    pub async fn start_server(&self, server: McpServer) -> Result<Vec<McpTool>, McpManagerError> {
        let server_id = server.id;

        // Check if already running
        {
            let servers = self.servers.read().await;
            if servers.contains_key(&server_id) {
                return Err(McpManagerError::AlreadyRunning(server_id.to_string()));
            }
        }

        // Start based on server type
        let (client, tools) = match server.server_type {
            McpServerType::Stdio => self.start_stdio_server(&server).await?,
            McpServerType::Sse => {
                // SSE not yet implemented
                return Err(McpManagerError::InvalidConfig(
                    "SSE servers not yet supported".to_string(),
                ));
            }
        };

        // Store running server
        {
            let mut servers = self.servers.write().await;
            servers.insert(
                server_id,
                RunningServer {
                    _server: server,
                    client,
                    status: McpServerStatus::Running,
                    tools: tools.clone(),
                },
            );
        }

        Ok(tools)
    }

    /// Start a stdio-based MCP server.
    async fn start_stdio_server(
        &self,
        server: &McpServer,
    ) -> Result<(McpClient, Vec<McpTool>), McpManagerError> {
        // Validate configuration
        server
            .config
            .validate(server.server_type)
            .map_err(McpManagerError::InvalidConfig)?;

        // For stdio servers, we expect a resolved absolute path in resolved_path_cache
        let exe_path = server
            .config
            .resolved_path_cache
            .as_ref()
            .or(server.config.command.as_ref())
            .ok_or_else(|| {
                McpManagerError::InvalidConfig(
                    "No executable path available (command not resolved)".to_string(),
                )
            })?;

        let args = server.config.args.as_deref().unwrap_or(&[]);
        let cwd = server.config.working_dir.as_deref();
        let path_extra = server.config.path_extra.as_deref();

        // Convert env entries to tuples
        let env: Vec<(String, String)> = server
            .env
            .iter()
            .map(|e| (e.key.clone(), e.value.clone()))
            .collect();

        let mut client = McpClient::new();

        // Connect and initialize
        client
            .connect_stdio(exe_path, args, cwd, path_extra, &env)
            .await
            .map_err(|e| McpManagerError::StartFailed(e.to_string()))?;

        // Discover tools
        let tools = client
            .list_tools()
            .await
            .map_err(|e| McpManagerError::StartFailed(format!("Failed to list tools: {e}")))?;

        tracing::info!(
            server_name = %server.name,
            tool_count = tools.len(),
            "MCP server started"
        );

        Ok((client, tools))
    }

    /// Stop an MCP server.
    pub async fn stop_server(&self, server_id: i64) -> Result<(), McpManagerError> {
        let mut server = {
            let mut servers = self.servers.write().await;
            servers
                .remove(&server_id)
                .ok_or_else(|| McpManagerError::NotRunning(server_id.to_string()))?
        };

        // Disconnect cleanly
        server.client.disconnect();
        server.status = McpServerStatus::Stopped;

        tracing::info!(server_id = %server_id, "MCP server stopped");

        Ok(())
    }

    /// Get the status of a server.
    pub async fn get_status(&self, server_id: i64) -> McpServerStatus {
        let servers = self.servers.read().await;

        servers
            .get(&server_id)
            .map_or(McpServerStatus::Stopped, |s| s.status.clone())
    }

    /// Get tools for a running server.
    pub async fn get_tools(&self, server_id: i64) -> Result<Vec<McpTool>, McpManagerError> {
        let servers = self.servers.read().await;
        let server = servers
            .get(&server_id)
            .ok_or_else(|| McpManagerError::NotRunning(server_id.to_string()))?;
        let tools = server.tools.clone();
        drop(servers);
        Ok(tools)
    }

    /// Get all tools from all running servers.
    pub async fn get_all_tools(&self) -> Vec<(i64, Vec<McpTool>)> {
        let servers = self.servers.read().await;

        servers
            .iter()
            .map(|(id, server)| (*id, server.tools.clone()))
            .collect()
    }

    /// Call a tool on a running server.
    pub async fn call_tool(
        &self,
        server_id: i64,
        tool_name: &str,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<McpToolResult, McpManagerError> {
        let servers = self.servers.read().await;
        let server = servers
            .get(&server_id)
            .ok_or_else(|| McpManagerError::NotRunning(server_id.to_string()))?;
        let result = server.client.call_tool(tool_name, arguments).await;
        drop(servers);

        result.map_err(std::convert::Into::into)
    }

    /// Check if a server is running.
    pub async fn is_running(&self, server_id: i64) -> bool {
        let servers = self.servers.read().await;
        servers.contains_key(&server_id)
    }

    /// Stop all running servers.
    pub async fn stop_all(&self) {
        let server_ids: Vec<i64> = {
            let servers = self.servers.read().await;
            servers.keys().copied().collect()
        };

        for id in server_ids {
            if let Err(e) = self.stop_server(id).await {
                tracing::warn!(server_id = %id, error = %e, "Failed to stop server");
            }
        }
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for McpManager {
    fn drop(&mut self) {
        // Note: We can't call async stop_all in Drop.
        // Servers will be cleaned up when their clients are dropped.
        // For proper cleanup, call stop_all() before dropping.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_not_running() {
        let manager = McpManager::new();
        let status = manager.get_status(999).await;
        assert_eq!(status, McpServerStatus::Stopped);
    }

    #[tokio::test]
    async fn test_stop_nonexistent_server() {
        let manager = McpManager::new();
        let result = manager.stop_server(999).await;
        assert!(matches!(result, Err(McpManagerError::NotRunning(_))));
    }
}
