//! High-level MCP service for managing MCP servers.
//!
//! This service provides the main API used by Tauri commands and REST endpoints.
//! It uses dependency injection for the repository and event emitter.

use crate::manager::McpManager;
use gglib_core::ports::{ResolutionAttempt, ResolutionStatus};
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
        // First, validate all servers and update their status
        self.validate_all_servers().await?;

        // Then start auto-start servers (skipping invalid ones)
        let servers = self.repository.list().await?;
        for server in servers {
            if server.auto_start && server.enabled && server.is_valid {
                if let Err(e) = self.start_server(server.id).await {
                    tracing::warn!(
                        server_name = %server.name,
                        error = %e,
                        "Failed to auto-start MCP server"
                    );
                }
            } else if server.auto_start && server.enabled && !server.is_valid {
                tracing::info!(
                    server_name = %server.name,
                    error = ?server.last_error,
                    "Skipping auto-start for invalid MCP server"
                );
            }
        }

        Ok(())
    }

    /// Validate all MCP servers and update their `is_valid/last_error` status.
    ///
    /// This checks:
    /// - Configuration validity (`exe_path/url` present based on type)
    /// - `exe_path` and `working_dir` are absolute paths
    /// - `exe_path` exists and is executable
    /// - `working_dir` exists if specified
    async fn validate_all_servers(&self) -> Result<(), McpServiceError> {
        let servers = self.repository.list().await?;

        for mut server in servers {
            let (is_valid, last_error) = match Self::validate_server(&server) {
                Ok(()) => (true, None),
                Err(e) => (false, Some(e)),
            };

            // Only update if status changed
            if server.is_valid != is_valid || server.last_error != last_error {
                server.is_valid = is_valid;
                server.last_error.clone_from(&last_error);

                if let Err(e) = self.repository.update(&server).await {
                    tracing::warn!(
                        server_id = server.id,
                        server_name = %server.name,
                        error = %e,
                        "Failed to update server validation status"
                    );
                }

                tracing::debug!(
                    server_id = server.id,
                    server_name = %server.name,
                    is_valid = is_valid,
                    error = ?last_error,
                    "Updated MCP server validation status"
                );
            }
        }

        Ok(())
    }

    /// Validate a single MCP server configuration and paths.
    fn validate_server(server: &McpServer) -> Result<(), String> {
        // Validate config structure
        server.config.validate(server.server_type)?;

        // For stdio servers, validate command and working directory
        if server.server_type == gglib_core::McpServerType::Stdio {
            // Ensure command has no whitespace (flags/args should be in args array)
            if let Some(ref cmd) = server.config.command {
                if cmd.contains(char::is_whitespace) {
                    return Err(
                        "Command must be an executable name/path only (e.g., 'npx'). \
                         Put flags and arguments in the 'args' field."
                            .to_string(),
                    );
                }
            }

            // Validate working directory if specified
            if let Some(ref cwd) = server.config.working_dir {
                if !cwd.is_empty() {
                    crate::path::validate_working_dir(cwd)?;
                }
            }
        }

        Ok(())
    }

    // =========================================================================
    // Path Resolution
    // =========================================================================

    /// Ensure a server's command is resolved to an absolute executable path.
    ///
    /// This method:
    /// - Checks if `resolved_path_cache` is still valid → returns it
    /// - Otherwise, resolves `command` using the path resolver
    /// - On success: updates `resolved_path_cache` in the database
    /// - On failure: preserves old cache, updates `is_valid`/`last_error`
    ///
    /// Returns a `ResolutionStatus` with success flag and diagnostic information.
    /// Resolution failure is not an error - it returns Ok(ResolutionStatus { success: false, ... })
    #[allow(clippy::too_many_lines)]
    pub async fn ensure_resolved(
        &self,
        server_id: i64,
    ) -> Result<ResolutionStatus, McpServiceError> {
        let mut server = self.repository.get_by_id(server_id).await?;

        // Only applicable to stdio servers
        if server.server_type != gglib_core::McpServerType::Stdio {
            return Ok(Self::stdio_only_error());
        }

        let command = match &server.config.command {
            Some(cmd) => cmd.clone(),
            None => return Ok(Self::no_command_error()),
        };

        // Step 1: Try cached resolved path first
        if let Some(cached_status) = Self::check_cached_path(&server) {
            return Ok(cached_status);
        }

        // Step 2: Cache miss or invalid - resolve from command
        let user_search_paths = Self::extract_user_search_paths(&server);

        match crate::resolver::resolve_executable(&command, &user_search_paths) {
            Ok(result) => {
                self.handle_resolution_success(&mut server, &command, result)
                    .await
            }
            Err(e) => {
                self.handle_resolution_failure(&mut server, &command, e)
                    .await
            }
        }
    }

    fn stdio_only_error() -> ResolutionStatus {
        ResolutionStatus {
            success: false,
            resolved_path: None,
            attempts: vec![],
            warnings: vec![],
            error_message: Some("Path resolution only applies to stdio servers".to_string()),
            suggested_fix: None,
        }
    }

    fn no_command_error() -> ResolutionStatus {
        ResolutionStatus {
            success: false,
            resolved_path: None,
            attempts: vec![],
            warnings: vec![],
            error_message: Some("No command specified".to_string()),
            suggested_fix: None,
        }
    }

    fn check_cached_path(server: &McpServer) -> Option<ResolutionStatus> {
        if let Some(ref cached_path) = server.config.resolved_path_cache {
            if crate::path::validate_exe_path(cached_path).is_ok() {
                tracing::debug!(
                    server_id = server.id,
                    server_name = %server.name,
                    cached_path = %cached_path,
                    "Using cached resolved path"
                );

                return Some(ResolutionStatus {
                    success: true,
                    resolved_path: Some(cached_path.clone()),
                    attempts: vec![ResolutionAttempt {
                        candidate: cached_path.clone(),
                        outcome: "OK (cached)".to_string(),
                    }],
                    warnings: vec![],
                    error_message: None,
                    suggested_fix: None,
                });
            }
        }
        None
    }

    fn extract_user_search_paths(server: &McpServer) -> Vec<String> {
        server
            .config
            .path_extra
            .as_ref()
            .map(|p| p.split(':').map(String::from).collect())
            .unwrap_or_default()
    }

    async fn handle_resolution_success(
        &self,
        server: &mut McpServer,
        command: &str,
        result: crate::resolver::ResolveResult,
    ) -> Result<ResolutionStatus, McpServiceError> {
        let resolved_path_str = result.resolved_path.to_string_lossy().to_string();
        server.config.resolved_path_cache = Some(resolved_path_str.clone());
        server.is_valid = true;
        server.last_error = None;

        if let Err(e) = self.repository.update(server).await {
            tracing::warn!(
                server_id = server.id,
                error = %e,
                "Failed to update resolved path cache"
            );
        }

        tracing::info!(
            server_id = server.id,
            server_name = %server.name,
            command = %command,
            resolved_path = %result.resolved_path.display(),
            "Successfully resolved command"
        );

        Ok(ResolutionStatus {
            success: true,
            resolved_path: Some(result.resolved_path.to_string_lossy().to_string()),
            attempts: result
                .attempts
                .into_iter()
                .map(|a| ResolutionAttempt {
                    candidate: a.candidate.to_string_lossy().to_string(),
                    outcome: a.outcome.to_string(),
                })
                .collect(),
            warnings: result.warnings,
            error_message: None,
            suggested_fix: None,
        })
    }

    async fn handle_resolution_failure(
        &self,
        server: &mut McpServer,
        command: &str,
        error: crate::resolver::ResolveError,
    ) -> Result<ResolutionStatus, McpServiceError> {
        let error_msg = error.to_string();
        server.is_valid = false;
        server.last_error = Some(error_msg.clone());

        if let Err(update_err) = self.repository.update(server).await {
            tracing::warn!(
                server_id = server.id,
                error = %update_err,
                "Failed to update server error state"
            );
        }

        tracing::warn!(
            server_id = server.id,
            server_name = %server.name,
            command = %command,
            error = %error,
            "Failed to resolve command"
        );

        let attempts = Self::extract_attempts_from_error(&error);
        let suggested_fix = Some(Self::generate_suggested_fix(command));

        Ok(ResolutionStatus {
            success: false,
            resolved_path: server.config.resolved_path_cache.clone(),
            attempts,
            warnings: vec![],
            error_message: Some(error_msg),
            suggested_fix,
        })
    }

    fn extract_attempts_from_error(
        error: &crate::resolver::ResolveError,
    ) -> Vec<ResolutionAttempt> {
        if let crate::resolver::ResolveError::NotResolved {
            attempts: err_attempts,
            ..
        } = error
        {
            err_attempts
                .lines()
                .filter(|line| line.trim().starts_with("✗"))
                .map(|line| {
                    let parts: Vec<&str> =
                        line.trim().trim_start_matches("✗").splitn(2, ':').collect();
                    ResolutionAttempt {
                        candidate: parts.first().unwrap_or(&"").trim().to_string(),
                        outcome: parts.get(1).unwrap_or(&"unknown").trim().to_string(),
                    }
                })
                .collect()
        } else {
            vec![]
        }
    }

    fn generate_suggested_fix(command: &str) -> String {
        if cfg!(windows) {
            format!("where {command}")
        } else {
            format!("command -v {command}")
        }
    }

    // =========================================================================
    // Configuration CRUD
    // =========================================================================

    /// Add a new MCP server configuration.
    pub async fn add_server(&self, new_server: NewMcpServer) -> Result<McpServer, McpServiceError> {
        let mut saved = self.repository.insert(new_server).await?;

        // Validate immediately after creation
        let (is_valid, last_error) = match Self::validate_server(&saved) {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e)),
        };

        saved.is_valid = is_valid;
        saved.last_error.clone_from(&last_error);

        // Update validation status in database
        if let Err(e) = self.repository.update(&saved).await {
            tracing::warn!(
                server_id = saved.id,
                server_name = %saved.name,
                error = %e,
                "Failed to update server validation status after creation"
            );
        }

        // Emit event
        self.emitter.emit(AppEvent::mcp_server_added(
            gglib_core::McpServerSummary::new(
                saved.id,
                saved.name.clone(),
                format!("{:?}", saved.server_type).to_lowercase(),
            ),
        ));

        tracing::info!(
            server_name = %saved.name,
            is_valid = is_valid,
            "Added MCP server configuration"
        );
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
    pub async fn update_server(&self, mut server: McpServer) -> Result<(), McpServiceError> {
        let id = server.id;

        // If server is running, stop it first
        if self.manager.is_running(id).await {
            self.manager.stop_server(id).await.map_err(|e| {
                McpServiceError::StopFailed(format!("Failed to stop before update: {e}"))
            })?;
        }

        // Validate before saving
        let (is_valid, last_error) = match Self::validate_server(&server) {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e)),
        };

        server.is_valid = is_valid;
        server.last_error = last_error;

        self.repository.update(&server).await?;
        tracing::info!(
            server_name = %server.name,
            is_valid = is_valid,
            "Updated MCP server configuration"
        );
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
        let mut server = self.repository.get_by_id(id).await?;
        let server_name = server.name.clone();

        // For stdio servers, ensure command is resolved before starting
        if server.server_type == gglib_core::McpServerType::Stdio {
            let status = self.ensure_resolved(id).await?;

            if !status.success {
                let error_msg = status.error_message_with_suggestions();
                tracing::error!(
                    server_id = id,
                    server_name = %server_name,
                    "Failed to resolve command before starting: {}",
                    error_msg
                );
                self.emitter
                    .emit(AppEvent::mcp_server_error(McpErrorInfo::process(
                        Some(id),
                        &server_name,
                        format!(
                            "Path resolution failed: {}",
                            status.error_message.unwrap_or_default()
                        ),
                    )));
                return Err(McpServiceError::InvalidConfig(error_msg));
            }

            // Update server with resolved path (already a String)
            if let Some(resolved_path) = status.resolved_path {
                server.config.resolved_path_cache = Some(resolved_path.clone());
                tracing::debug!(
                    server_id = id,
                    server_name = %server_name,
                    resolved_path = %resolved_path,
                    "Resolved path before starting server"
                );
            }
        }

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
            is_valid: true,
            last_error: None,
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
            let id = {
                let mut next_id = self.next_id.lock().unwrap();
                let id = *next_id;
                *next_id += 1;
                id
            };

            let server = McpServer {
                id,
                name: new_server.name,
                server_type: new_server.server_type,
                config: new_server.config,
                enabled: new_server.enabled,
                auto_start: new_server.auto_start,
                env: new_server.env,
                created_at: chrono::Utc::now(),
                last_connected_at: None,
                is_valid: false,
                last_error: None,
            };

            self.servers.lock().unwrap().push(server.clone());
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
            servers.iter_mut().find(|s| s.id == server.id).map_or_else(
                || Err(McpRepositoryError::NotFound(server.id.to_string())),
                |s| {
                    *s = server.clone();
                    Ok(())
                },
            )
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

        let new_server = NewMcpServer::new_stdio("Test", "echo", vec!["hello".to_string()], None);
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

        let new_server = NewMcpServer::new_stdio("Test", "echo", vec![], None);
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

        let new_server = NewMcpServer::new_stdio("Test", "echo", vec![], None);
        let saved = service.add_server(new_server).await.unwrap();

        let status = service.get_server_status(saved.id).await;
        assert_eq!(status, McpServerStatus::Stopped);
    }
}
