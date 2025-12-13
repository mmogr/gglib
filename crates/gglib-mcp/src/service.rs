//! High-level MCP service for managing MCP servers.
//!
//! This service provides the main API used by Tauri commands and REST endpoints.
//! It uses dependency injection for the repository and event emitter.

use crate::manager::McpManager;
use gglib_core::{
    AppEvent, AppEventEmitter, McpErrorInfo, McpServer, McpServerRepository, McpServerStatus,
    McpServiceError, McpTool, McpToolResult, NewMcpServer,
};
use std::collections::HashMap;
use std::sync::Arc;

/// Server info with runtime status and tools.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpServerInfo {
    /// Server configuration
    pub server: McpServer,
    /// Current runtime status
    pub status: McpServerStatus,
    /// Tools exposed by this server (populated when running)
    #[serde(default)]
    pub tools: Vec<McpTool>,
}

/// MCP service providing unified access to MCP server management.
///
/// This is the main interface used by Tauri commands and REST API.
/// It uses dependency injection for testability and clean architecture.
pub struct McpService {
    repository: Arc<dyn McpServerRepository>,
    manager: Arc<McpManager>,
    emitter: Arc<dyn AppEventEmitter>,
}

impl McpService {
    /// Create a new MCP service with injected dependencies.
    pub fn new(
        repository: Arc<dyn McpServerRepository>,
        emitter: Arc<dyn AppEventEmitter>,
    ) -> Self {
        Self {
            repository,
            manager: Arc::new(McpManager::new()),
            emitter,
        }
    }

    /// Initialize the MCP service (starts auto-start servers).
    pub async fn initialize(&self) -> Result<(), McpServiceError> {
        // Start auto-start servers
        let servers = self.repository.list().await?;
        for server in servers {
            if server.auto_start && server.enabled {
                if let Err(e) = self.start_server(server.id).await {
                    tracing::warn!(
                        server_name = %server.name,
                        error = %e,
                        "Failed to auto-start MCP server"
                    );
                }
            }
        }

        Ok(())
    }

    // =========================================================================
    // Configuration CRUD
    // =========================================================================

    /// Add a new MCP server configuration.
    pub async fn add_server(&self, new_server: NewMcpServer) -> Result<McpServer, McpServiceError> {
        let saved = self.repository.insert(new_server).await?;

        // Emit event
        self.emitter.emit(AppEvent::mcp_server_added(
            gglib_core::McpServerSummary::new(
                saved.id,
                saved.name.clone(),
                format!("{:?}", saved.server_type).to_lowercase(),
            ),
        ));

        tracing::info!(server_name = %saved.name, "Added MCP server configuration");
        Ok(saved)
    }

    /// Get a server configuration by ID.
    pub async fn get_server(&self, id: i64) -> Result<McpServer, McpServiceError> {
        Ok(self.repository.get_by_id(id).await?)
    }

    /// Get a server configuration by name.
    pub async fn get_server_by_name(&self, name: &str) -> Result<McpServer, McpServiceError> {
        Ok(self.repository.get_by_name(name).await?)
    }

    /// List all server configurations.
    pub async fn list_servers(&self) -> Result<Vec<McpServer>, McpServiceError> {
        Ok(self.repository.list().await?)
    }

    /// Update a server configuration.
    pub async fn update_server(&self, server: McpServer) -> Result<(), McpServiceError> {
        let id = server.id;

        // If server is running, stop it first
        if self.manager.is_running(id).await {
            self.manager.stop_server(id).await.map_err(|e| {
                McpServiceError::StopFailed(format!("Failed to stop before update: {e}"))
            })?;
        }

        self.repository.update(&server).await?;
        tracing::info!(server_name = %server.name, "Updated MCP server configuration");
        Ok(())
    }

    /// Remove a server configuration.
    pub async fn remove_server(&self, id: i64) -> Result<(), McpServiceError> {
        // Stop if running
        if self.manager.is_running(id).await {
            self.manager
                .stop_server(id)
                .await
                .map_err(|e| McpServiceError::StopFailed(e.to_string()))?;
        }

        self.repository.delete(id).await?;

        // Emit event
        self.emitter.emit(AppEvent::mcp_server_removed(id));

        tracing::info!(server_id = %id, "Removed MCP server configuration");
        Ok(())
    }

    // =========================================================================
    // Server Lifecycle
    // =========================================================================

    /// Start an MCP server.
    pub async fn start_server(&self, id: i64) -> Result<Vec<McpTool>, McpServiceError> {
        let server = self.repository.get_by_id(id).await?;
        let server_name = server.name.clone();

        let tools = self.manager.start_server(server).await.map_err(|e| {
            // Emit error event
            self.emitter
                .emit(AppEvent::mcp_server_error(McpErrorInfo::process(
                    Some(id),
                    &server_name,
                    e.to_string(),
                )));
            McpServiceError::StartFailed(e.to_string())
        })?;

        // Update last connected timestamp
        let _ = self.repository.update_last_connected(id).await;

        // Emit started event
        self.emitter
            .emit(AppEvent::mcp_server_started(id, &server_name));

        Ok(tools)
    }

    /// Stop an MCP server.
    pub async fn stop_server(&self, id: i64) -> Result<(), McpServiceError> {
        // Get server name for event before stopping
        let server_name = match self.repository.get_by_id(id).await {
            Ok(s) => s.name,
            Err(_) => format!("server-{id}"),
        };

        self.manager
            .stop_server(id)
            .await
            .map_err(|e| McpServiceError::StopFailed(e.to_string()))?;

        // Emit stopped event
        self.emitter
            .emit(AppEvent::mcp_server_stopped(id, server_name));

        Ok(())
    }

    /// Get the status of a server.
    pub async fn get_server_status(&self, id: i64) -> McpServerStatus {
        self.manager.get_status(id).await
    }

    /// Get full server info including runtime status and tools.
    pub async fn get_server_info(&self, id: i64) -> Result<McpServerInfo, McpServiceError> {
        let server = self.repository.get_by_id(id).await?;
        let status = self.manager.get_status(id).await;
        let tools = if status == McpServerStatus::Running {
            self.manager.get_tools(id).await.unwrap_or_default()
        } else {
            Vec::new()
        };

        Ok(McpServerInfo {
            server,
            status,
            tools,
        })
    }

    /// List all servers with their runtime status.
    pub async fn list_servers_with_status(&self) -> Result<Vec<McpServerInfo>, McpServiceError> {
        let servers = self.repository.list().await?;
        let mut infos = Vec::with_capacity(servers.len());

        for server in servers {
            let id = server.id;
            let status = self.manager.get_status(id).await;
            let tools = if status == McpServerStatus::Running {
                self.manager.get_tools(id).await.unwrap_or_default()
            } else {
                Vec::new()
            };

            infos.push(McpServerInfo {
                server,
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
    pub async fn list_server_tools(&self, id: i64) -> Result<Vec<McpTool>, McpServiceError> {
        self.manager
            .get_tools(id)
            .await
            .map_err(|e| McpServiceError::NotRunning(e.to_string()))
    }

    /// Get all tools from all running servers.
    pub async fn list_all_tools(&self) -> Vec<(i64, Vec<McpTool>)> {
        self.manager.get_all_tools().await
    }

    /// Call a tool on a server.
    pub async fn call_tool(
        &self,
        server_id: i64,
        tool_name: &str,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<McpToolResult, McpServiceError> {
        self.manager
            .call_tool(server_id, tool_name, arguments)
            .await
            .map_err(|e| McpServiceError::ToolError(e.to_string()))
    }

    // =========================================================================
    // Utilities
    // =========================================================================

    /// Stop all running servers.
    pub async fn shutdown(&self) {
        self.manager.stop_all().await;
    }

    /// Test connection to a server configuration (starts, gets tools, then stops).
    pub async fn test_connection(
        &self,
        new_server: NewMcpServer,
    ) -> Result<Vec<McpTool>, McpServiceError> {
        // Create a temporary server with fake ID for testing
        let test_server = McpServer {
            id: -1,
            name: new_server.name,
            server_type: new_server.server_type,
            config: new_server.config,
            enabled: new_server.enabled,
            auto_start: new_server.auto_start,
            env: new_server.env,
            created_at: chrono::Utc::now(),
            last_connected_at: None,
        };

        let tools = self
            .manager
            .start_server(test_server)
            .await
            .map_err(|e| McpServiceError::StartFailed(e.to_string()))?;

        // Stop the test server
        self.manager
            .stop_server(-1)
            .await
            .map_err(|e| McpServiceError::StopFailed(e.to_string()))?;

        Ok(tools)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use gglib_core::{McpRepositoryError, NoopEmitter};
    use std::sync::Mutex;

    /// Mock repository for testing
    struct MockMcpRepository {
        servers: Mutex<Vec<McpServer>>,
        next_id: Mutex<i64>,
    }

    impl MockMcpRepository {
        fn new() -> Self {
            Self {
                servers: Mutex::new(Vec::new()),
                next_id: Mutex::new(1),
            }
        }
    }

    #[async_trait]
    impl McpServerRepository for MockMcpRepository {
        async fn insert(&self, new_server: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
            let mut servers = self.servers.lock().unwrap();
            let mut next_id = self.next_id.lock().unwrap();

            let server = McpServer {
                id: *next_id,
                name: new_server.name,
                server_type: new_server.server_type,
                config: new_server.config,
                enabled: new_server.enabled,
                auto_start: new_server.auto_start,
                env: new_server.env,
                created_at: chrono::Utc::now(),
                last_connected_at: None,
            };

            *next_id += 1;
            servers.push(server.clone());
            Ok(server)
        }

        async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
            let servers = self.servers.lock().unwrap();
            servers
                .iter()
                .find(|s| s.id == id)
                .cloned()
                .ok_or_else(|| McpRepositoryError::NotFound(id.to_string()))
        }

        async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
            let servers = self.servers.lock().unwrap();
            servers
                .iter()
                .find(|s| s.name == name)
                .cloned()
                .ok_or_else(|| McpRepositoryError::NotFound(name.to_string()))
        }

        async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
            let servers = self.servers.lock().unwrap();
            Ok(servers.clone())
        }

        async fn update(&self, server: &McpServer) -> Result<(), McpRepositoryError> {
            let mut servers = self.servers.lock().unwrap();
            if let Some(s) = servers.iter_mut().find(|s| s.id == server.id) {
                *s = server.clone();
                Ok(())
            } else {
                Err(McpRepositoryError::NotFound(server.id.to_string()))
            }
        }

        async fn delete(&self, id: i64) -> Result<(), McpRepositoryError> {
            let mut servers = self.servers.lock().unwrap();
            let len_before = servers.len();
            servers.retain(|s| s.id != id);
            if servers.len() < len_before {
                Ok(())
            } else {
                Err(McpRepositoryError::NotFound(id.to_string()))
            }
        }

        async fn update_last_connected(&self, id: i64) -> Result<(), McpRepositoryError> {
            let mut servers = self.servers.lock().unwrap();
            if let Some(s) = servers.iter_mut().find(|s| s.id == id) {
                s.last_connected_at = Some(chrono::Utc::now());
                Ok(())
            } else {
                Err(McpRepositoryError::NotFound(id.to_string()))
            }
        }
    }

    #[tokio::test]
    async fn test_add_and_list_servers() {
        let repo = Arc::new(MockMcpRepository::new());
        let emitter = Arc::new(NoopEmitter::new());
        let service = McpService::new(repo, emitter);

        let new_server = NewMcpServer::new_stdio("Test", "echo", vec!["hello".to_string()]);
        let saved = service.add_server(new_server).await.unwrap();
        assert!(saved.id > 0);

        let servers = service.list_servers().await.unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "Test");
    }

    #[tokio::test]
    async fn test_remove_server() {
        let repo = Arc::new(MockMcpRepository::new());
        let emitter = Arc::new(NoopEmitter::new());
        let service = McpService::new(repo, emitter);

        let new_server = NewMcpServer::new_stdio("Test", "echo", vec![]);
        let saved = service.add_server(new_server).await.unwrap();

        service.remove_server(saved.id).await.unwrap();

        let result = service.get_server(saved.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_server_status_when_not_running() {
        let repo = Arc::new(MockMcpRepository::new());
        let emitter = Arc::new(NoopEmitter::new());
        let service = McpService::new(repo, emitter);

        let new_server = NewMcpServer::new_stdio("Test", "echo", vec![]);
        let saved = service.add_server(new_server).await.unwrap();

        let status = service.get_server_status(saved.id).await;
        assert_eq!(status, McpServerStatus::Stopped);
    }
}
