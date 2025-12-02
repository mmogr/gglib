//! High-level MCP service for managing MCP servers.
//!
//! This service provides the main API used by Tauri commands and REST endpoints.

use super::config::{McpServerConfig, McpServerInfo, McpServerStatus, McpTool, McpToolResult};
use super::database::{McpDatabase, McpDatabaseError};
use super::manager::{McpManager, McpManagerError};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Errors that can occur in the MCP service.
#[derive(Debug, Error)]
pub enum McpServiceError {
    #[error("Database error: {0}")]
    Database(#[from] McpDatabaseError),

    #[error("Manager error: {0}")]
    Manager(#[from] McpManagerError),

    #[error("Server not found: {0}")]
    NotFound(String),
}

impl From<McpServiceError> for String {
    fn from(e: McpServiceError) -> String {
        e.to_string()
    }
}

/// MCP service providing unified access to MCP server management.
///
/// This is the main interface used by Tauri commands and REST API.
pub struct McpService {
    database: McpDatabase,
    manager: Arc<McpManager>,
}

impl McpService {
    /// Create a new MCP service.
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            database: McpDatabase::new(pool),
            manager: Arc::new(McpManager::new()),
        }
    }

    /// Initialize the MCP service (creates schema, starts auto-start servers).
    pub async fn initialize(&self) -> Result<(), McpServiceError> {
        // Ensure database schema exists
        self.database.ensure_schema().await?;

        // Start auto-start servers
        let servers = self.database.list_servers().await?;
        for server in servers {
            if server.auto_start
                && server.enabled
                && let Err(e) = self.start_server(&server.id.unwrap().to_string()).await
            {
                tracing::warn!(
                    server_name = %server.name,
                    error = %e,
                    "Failed to auto-start MCP server"
                );
            }
        }

        Ok(())
    }

    // =========================================================================
    // Configuration CRUD
    // =========================================================================

    /// Add a new MCP server configuration.
    pub async fn add_server(
        &self,
        config: McpServerConfig,
    ) -> Result<McpServerConfig, McpServiceError> {
        let saved = self.database.add_server(config).await?;
        tracing::info!(server_name = %saved.name, "Added MCP server configuration");
        Ok(saved)
    }

    /// Get a server configuration by ID.
    pub async fn get_server(&self, id: &str) -> Result<McpServerConfig, McpServiceError> {
        let id: i64 = id
            .parse()
            .map_err(|_| McpServiceError::NotFound(id.to_string()))?;
        Ok(self.database.get_server(id).await?)
    }

    /// List all server configurations.
    pub async fn list_servers(&self) -> Result<Vec<McpServerConfig>, McpServiceError> {
        Ok(self.database.list_servers().await?)
    }

    /// Update a server configuration.
    pub async fn update_server(
        &self,
        id: &str,
        config: McpServerConfig,
    ) -> Result<McpServerConfig, McpServiceError> {
        let id: i64 = id
            .parse()
            .map_err(|_| McpServiceError::NotFound(id.to_string()))?;

        // If server is running, stop it first
        if self.manager.is_running(&id.to_string()).await {
            self.manager.stop_server(&id.to_string()).await?;
        }

        let updated = self.database.update_server(id, config).await?;
        tracing::info!(server_name = %updated.name, "Updated MCP server configuration");
        Ok(updated)
    }

    /// Remove a server configuration.
    pub async fn remove_server(&self, id: &str) -> Result<(), McpServiceError> {
        let id_num: i64 = id
            .parse()
            .map_err(|_| McpServiceError::NotFound(id.to_string()))?;

        // Stop if running
        if self.manager.is_running(id).await {
            self.manager.stop_server(id).await?;
        }

        self.database.remove_server(id_num).await?;
        tracing::info!(server_id = %id, "Removed MCP server configuration");
        Ok(())
    }

    // =========================================================================
    // Server Lifecycle
    // =========================================================================

    /// Start an MCP server.
    pub async fn start_server(&self, id: &str) -> Result<Vec<McpTool>, McpServiceError> {
        let config = self.get_server(id).await?;

        let tools = self.manager.start_server(config).await?;

        // Update last connected timestamp
        if let Ok(id_num) = id.parse::<i64>() {
            let _ = self.database.update_last_connected(id_num).await;
        }

        Ok(tools)
    }

    /// Stop an MCP server.
    pub async fn stop_server(&self, id: &str) -> Result<(), McpServiceError> {
        self.manager.stop_server(id).await?;
        Ok(())
    }

    /// Get the status of a server.
    pub async fn get_server_status(&self, id: &str) -> McpServerStatus {
        self.manager.get_status(id).await
    }

    /// Get full server info including runtime status and tools.
    pub async fn get_server_info(&self, id: &str) -> Result<McpServerInfo, McpServiceError> {
        let config = self.get_server(id).await?;
        let status = self.manager.get_status(id).await;
        let tools = if status == McpServerStatus::Running {
            self.manager.get_tools(id).await.unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(McpServerInfo {
            config,
            status,
            tools,
        })
    }

    /// List all servers with their runtime status.
    pub async fn list_servers_with_status(&self) -> Result<Vec<McpServerInfo>, McpServiceError> {
        let configs = self.database.list_servers().await?;
        let mut infos = Vec::with_capacity(configs.len());

        for config in configs {
            let id = config.id.map(|i| i.to_string()).unwrap_or_default();
            let status = self.manager.get_status(&id).await;
            let tools = if status == McpServerStatus::Running {
                self.manager.get_tools(&id).await.unwrap_or_default()
            } else {
                Vec::new()
            };

            infos.push(McpServerInfo {
                config,
                status,
                tools,
            });
        }

        Ok(infos)
    }

    // =========================================================================
    // Tool Operations
    // =========================================================================

    /// List tools for a specific server.
    pub async fn list_server_tools(&self, id: &str) -> Result<Vec<McpTool>, McpServiceError> {
        Ok(self.manager.get_tools(id).await?)
    }

    /// Get all tools from all running servers.
    pub async fn list_all_tools(&self) -> Vec<(String, Vec<McpTool>)> {
        self.manager.get_all_tools().await
    }

    /// Call a tool on a server.
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<McpToolResult, McpServiceError> {
        Ok(self
            .manager
            .call_tool(server_id, tool_name, arguments)
            .await?)
    }

    // =========================================================================
    // Utilities
    // =========================================================================

    /// Stop all running servers.
    pub async fn shutdown(&self) {
        self.manager.stop_all().await;
    }

    /// Test connection to a server (starts, gets tools, then stops).
    pub async fn test_connection(
        &self,
        config: McpServerConfig,
    ) -> Result<Vec<McpTool>, McpServiceError> {
        // Create a temporary config with a fake ID for testing
        let mut test_config = config;
        test_config.id = Some(-1); // Use negative ID to indicate test

        let tools = self.manager.start_server(test_config).await?;

        // Stop the test server
        self.manager.stop_server("-1").await?;

        Ok(tools)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_service() -> McpService {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let service = McpService::new(pool);
        service.database.ensure_schema().await.unwrap();
        service
    }

    #[tokio::test]
    async fn test_add_and_list_servers() {
        let service = setup_test_service().await;

        let config = McpServerConfig::new_stdio("Test", "echo", vec!["hello".to_string()]);
        let saved = service.add_server(config).await.unwrap();
        assert!(saved.id.is_some());

        let servers = service.list_servers().await.unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "Test");
    }

    #[tokio::test]
    async fn test_remove_server() {
        let service = setup_test_service().await;

        let config = McpServerConfig::new_stdio("Test", "echo", vec![]);
        let saved = service.add_server(config).await.unwrap();
        let id = saved.id.unwrap().to_string();

        service.remove_server(&id).await.unwrap();

        let result = service.get_server(&id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_server_status_when_not_running() {
        let service = setup_test_service().await;

        let config = McpServerConfig::new_stdio("Test", "echo", vec![]);
        let saved = service.add_server(config).await.unwrap();
        let id = saved.id.unwrap().to_string();

        let status = service.get_server_status(&id).await;
        assert_eq!(status, McpServerStatus::Stopped);
    }
}
