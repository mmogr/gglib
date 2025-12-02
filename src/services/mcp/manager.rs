//! MCP server lifecycle management.
//!
//! Manages starting, stopping, and monitoring MCP server processes.

use super::client::McpClient;
use super::config::{McpServerConfig, McpServerStatus, McpServerType, McpTool, McpToolResult};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Errors that can occur during MCP manager operations.
#[derive(Debug, Error)]
pub enum McpManagerError {
    #[error("Server not found: {0}")]
    ServerNotFound(String),

    #[error("Server already running: {0}")]
    AlreadyRunning(String),

    #[error("Server not running: {0}")]
    NotRunning(String),

    #[error("Failed to start server: {0}")]
    StartFailed(String),

    #[error("Client error: {0}")]
    ClientError(#[from] super::client::McpClientError),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Running MCP server instance.
struct RunningServer {
    /// Server configuration
    config: McpServerConfig,
    /// MCP client for communication
    client: McpClient,
    /// Current status
    status: McpServerStatus,
    /// Discovered tools
    tools: Vec<McpTool>,
}

/// Manager for MCP server lifecycle.
pub struct McpManager {
    /// Running servers indexed by server ID
    servers: Arc<RwLock<HashMap<String, RunningServer>>>,
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
    pub async fn start_server(
        &self,
        config: McpServerConfig,
    ) -> Result<Vec<McpTool>, McpManagerError> {
        let server_id = config
            .id
            .map(|id| id.to_string())
            .ok_or_else(|| McpManagerError::InvalidConfig("Server has no ID".to_string()))?;

        // Check if already running
        {
            let servers = self.servers.read().await;
            if servers.contains_key(&server_id) {
                return Err(McpManagerError::AlreadyRunning(server_id));
            }
        }

        // Start based on server type
        let (client, tools) = match config.server_type {
            McpServerType::Stdio => self.start_stdio_server(&config).await?,
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
                server_id.clone(),
                RunningServer {
                    config,
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
        config: &McpServerConfig,
    ) -> Result<(McpClient, Vec<McpTool>), McpManagerError> {
        let command = config.command.as_ref().ok_or_else(|| {
            McpManagerError::InvalidConfig("Stdio server requires command".to_string())
        })?;

        let args = config.args.as_ref().map(|a| a.as_slice()).unwrap_or(&[]);
        let cwd = config.cwd.as_deref();
        let env: Vec<(String, String)> = config.env.clone();

        let mut client = McpClient::new();

        // Connect and initialize
        client
            .connect_stdio(command, args, cwd, &env)
            .await
            .map_err(|e| McpManagerError::StartFailed(e.to_string()))?;

        // Discover tools
        let tools = client
            .list_tools()
            .await
            .map_err(|e| McpManagerError::StartFailed(format!("Failed to list tools: {}", e)))?;

        tracing::info!(
            server_name = %config.name,
            tool_count = tools.len(),
            "MCP server started"
        );

        Ok((client, tools))
    }

    /// Stop an MCP server.
    pub async fn stop_server(&self, server_id: &str) -> Result<(), McpManagerError> {
        let mut servers = self.servers.write().await;

        let mut server = servers
            .remove(server_id)
            .ok_or_else(|| McpManagerError::NotRunning(server_id.to_string()))?;

        // Disconnect cleanly
        server.client.disconnect();
        server.status = McpServerStatus::Stopped;

        tracing::info!(server_id = %server_id, "MCP server stopped");

        Ok(())
    }

    /// Get the status of a server.
    pub async fn get_status(&self, server_id: &str) -> McpServerStatus {
        let servers = self.servers.read().await;

        servers
            .get(server_id)
            .map(|s| s.status.clone())
            .unwrap_or(McpServerStatus::Stopped)
    }

    /// Get tools for a running server.
    pub async fn get_tools(&self, server_id: &str) -> Result<Vec<McpTool>, McpManagerError> {
        let servers = self.servers.read().await;

        let server = servers
            .get(server_id)
            .ok_or_else(|| McpManagerError::NotRunning(server_id.to_string()))?;

        Ok(server.tools.clone())
    }

    /// Get all tools from all running servers.
    pub async fn get_all_tools(&self) -> Vec<(String, Vec<McpTool>)> {
        let servers = self.servers.read().await;

        servers
            .iter()
            .map(|(id, server)| (id.clone(), server.tools.clone()))
            .collect()
    }

    /// Call a tool on a running server.
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<McpToolResult, McpManagerError> {
        // Get a reference to call the tool
        // Note: We need write lock because client.call_tool may modify internal state
        let servers = self.servers.read().await;

        let server = servers
            .get(server_id)
            .ok_or_else(|| McpManagerError::NotRunning(server_id.to_string()))?;

        server
            .client
            .call_tool(tool_name, arguments)
            .await
            .map_err(|e| e.into())
    }

    /// Check if a server is running.
    pub async fn is_running(&self, server_id: &str) -> bool {
        let servers = self.servers.read().await;
        servers.contains_key(server_id)
    }

    /// Get list of running server IDs.
    pub async fn running_servers(&self) -> Vec<String> {
        let servers = self.servers.read().await;
        servers.keys().cloned().collect()
    }

    /// Stop all running servers.
    pub async fn stop_all(&self) {
        let server_ids: Vec<String> = {
            let servers = self.servers.read().await;
            servers.keys().cloned().collect()
        };

        for id in server_ids {
            if let Err(e) = self.stop_server(&id).await {
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
    async fn test_manager_creation() {
        let manager = McpManager::new();
        assert!(manager.running_servers().await.is_empty());
    }

    #[tokio::test]
    async fn test_server_not_running() {
        let manager = McpManager::new();
        let status = manager.get_status("nonexistent").await;
        assert_eq!(status, McpServerStatus::Stopped);
    }

    #[tokio::test]
    async fn test_stop_nonexistent_server() {
        let manager = McpManager::new();
        let result = manager.stop_server("nonexistent").await;
        assert!(matches!(result, Err(McpManagerError::NotRunning(_))));
    }
}
