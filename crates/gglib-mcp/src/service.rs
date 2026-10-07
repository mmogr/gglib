//! High-level MCP service for managing MCP servers.
//!
//! This service provides the main API used by Tauri commands and REST endpoints.
//! It uses dependency injection for the repository.

use crate::manager::McpManager;
use gglib_core::ports::{ResolutionAttempt, ResolutionStatus};
use gglib_core::{
    McpLifecycle, McpRepositoryError, McpServer, McpServerRepository, McpServerStatus,
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
    /// Held from the check that a name is free to the write that takes it,
    /// so two requests for one name cannot both pass the check.
    naming: tokio::sync::Mutex<()>,
}

impl McpService {
    /// Create a new MCP service with injected dependencies.
    pub fn new(repository: Arc<dyn McpServerRepository>) -> Self {
        Self {
            repository,
            manager: Arc::new(McpManager::new()),
            naming: tokio::sync::Mutex::new(()),
        }
    }

    /// Initialize the MCP service: validates all servers and starts `Eager` ones.
    ///
    /// `Lazy` servers start on first tool use (see `ensure_started_for_call`).
    /// `Manual` servers are never started automatically.
    /// For one-shot commands that need all tools available immediately, call
    /// `prewarm_lazy` immediately after this.
    pub async fn initialize(&self) -> Result<(), McpServiceError> {
        // First, validate all servers and update their status
        self.validate_all_servers().await?;

        // Start only Eager servers (Lazy: on-demand, Manual: never).
        let servers = self.repository.list().await?;
        for server in servers {
            if server.lifecycle == McpLifecycle::Eager && server.enabled && server.is_valid {
                if let Err(e) = self.start_server(server.id).await {
                    tracing::warn!(
                        server_name = %server.name,
                        error = %e,
                        "Failed to start eager MCP server"
                    );
                }
            } else if server.lifecycle == McpLifecycle::Eager && server.enabled && !server.is_valid
            {
                tracing::info!(
                    server_name = %server.name,
                    error = ?server.last_error,
                    "Skipping eager start for invalid MCP server"
                );
            }
        }

        Ok(())
    }

    /// Pre-warm all `Lazy` servers.
    ///
    /// One-shot CLI commands (`gglib q`) call this
    /// right after `initialize` so that tools are ready before generation begins,
    /// avoiding per-tool spawn latency mid-stream.
    pub async fn prewarm_lazy(&self) {
        let servers = match self.repository.list().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to list servers for lazy prewarm");
                return;
            }
        };

        for server in servers {
            if server.lifecycle == McpLifecycle::Lazy && server.enabled && server.is_valid {
                if let Err(e) = self.start_server(server.id).await {
                    tracing::warn!(
                        server_name = %server.name,
                        error = %e,
                        "Failed to prewarm lazy MCP server"
                    );
                }
            }
        }
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
            // Only update if status changed
            if Self::stamp_validity(&mut server) {
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
                    is_valid = server.is_valid,
                    error = ?server.last_error,
                    "Updated MCP server validation status"
                );
            }
        }

        Ok(())
    }

    /// Validate `server` and record the verdict on it, in `is_valid` and
    /// `last_error`. Returns whether that changed either.
    fn stamp_validity(server: &mut McpServer) -> bool {
        let (is_valid, last_error) = match Self::validate_server(server) {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e)),
        };

        let changed = server.is_valid != is_valid || server.last_error != last_error;
        server.is_valid = is_valid;
        server.last_error = last_error;
        changed
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
    #[allow(clippy::cognitive_complexity)]
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
            .map(|p| {
                p.split(crate::resolver::PATH_SEPARATOR)
                    .map(String::from)
                    .collect()
            })
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

    /// Refuse `name` when a server already has it.
    ///
    /// The rule that names are unique lives here and not in the schema: a
    /// database written before it was enforced can hold two servers of one
    /// name, and must still open. The caller holds `naming`.
    async fn refuse_taken_name(&self, name: &str) -> Result<(), McpServiceError> {
        match self.repository.get_by_name(name).await {
            Ok(_) => Err(McpServiceError::NameTaken(name.to_string())),
            Err(McpRepositoryError::NotFound(_)) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Add a new MCP server configuration.
    ///
    /// Refused with [`McpServiceError::NameTaken`] when a server already has
    /// the name.
    pub async fn add_server(&self, new_server: NewMcpServer) -> Result<McpServer, McpServiceError> {
        let _naming = self.naming.lock().await;
        self.refuse_taken_name(&new_server.name).await?;

        let mut saved = self.repository.insert(new_server).await?;

        // Validate immediately after creation
        Self::stamp_validity(&mut saved);

        // Update validation status in database
        if let Err(e) = self.repository.update(&saved).await {
            tracing::warn!(
                server_id = saved.id,
                server_name = %saved.name,
                error = %e,
                "Failed to update server validation status after creation"
            );
        }

        tracing::info!(
            server_name = %saved.name,
            is_valid = saved.is_valid,
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
    ///
    /// A rename to a name another server has is refused with
    /// [`McpServiceError::NameTaken`], before anything is stopped or
    /// written. A server keeps the name it has, even one it shares.
    pub async fn update_server(&self, mut server: McpServer) -> Result<(), McpServiceError> {
        let id = server.id;

        let _naming = self.naming.lock().await;
        if self.repository.get_by_id(id).await?.name != server.name {
            self.refuse_taken_name(&server.name).await?;
        }

        // If server is running, stop it first
        if self.manager.is_running(id).await {
            self.manager.stop_server(id).await.map_err(|e| {
                McpServiceError::StopFailed(format!("Failed to stop before update: {e}"))
            })?;
        }

        // Validate before saving
        Self::stamp_validity(&mut server);

        self.repository.update(&server).await?;
        tracing::info!(
            server_name = %server.name,
            is_valid = server.is_valid,
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

        tracing::info!(server_id = %id, "Removed MCP server configuration");
        Ok(())
    }

    // =========================================================================
    // Server Lifecycle
    // =========================================================================

    /// Start an MCP server.
    pub async fn start_server(&self, id: i64) -> Result<Vec<McpTool>, McpServiceError> {
        let mut server = self.repository.get_by_id(id).await?;

        // For stdio servers, ensure command is resolved before starting
        if server.server_type == gglib_core::McpServerType::Stdio {
            let status = self.ensure_resolved(id).await?;

            if !status.success {
                let error_msg = status.error_message_with_suggestions();
                tracing::error!(
                    server_id = id,
                    server_name = %server.name,
                    "Failed to resolve command before starting: {}",
                    error_msg
                );
                return Err(McpServiceError::InvalidConfig(error_msg));
            }

            // Update server with resolved path (already a String)
            if let Some(resolved_path) = status.resolved_path {
                server.config.resolved_path_cache = Some(resolved_path.clone());
                tracing::debug!(
                    server_id = id,
                    server_name = %server.name,
                    resolved_path = %resolved_path,
                    "Resolved path before starting server"
                );
            }
        }

        let tools = self
            .manager
            .start_server(&server)
            .await
            .map_err(|e| McpServiceError::StartFailed(e.to_string()))?;

        // Update last connected timestamp
        let _ = self.repository.update_last_connected(id).await;

        Ok(tools)
    }

    /// Stop an MCP server.
    pub async fn stop_server(&self, id: i64) -> Result<(), McpServiceError> {
        self.manager
            .stop_server(id)
            .await
            .map_err(|e| McpServiceError::StopFailed(e.to_string()))?;

        Ok(())
    }

    /// Get the status of a server.
    pub async fn get_server_status(&self, id: i64) -> McpServerStatus {
        self.manager.get_status(id).await
    }

    /// A server with its runtime status, and its tools while it runs.
    async fn info_for(&self, server: McpServer) -> McpServerInfo {
        let status = self.manager.get_status(server.id).await;
        let tools = if status == McpServerStatus::Running {
            self.manager.get_tools(server.id).await.unwrap_or_default()
        } else {
            Vec::new()
        };

        McpServerInfo {
            server,
            status,
            tools,
        }
    }

    /// Get full server info including runtime status and tools.
    pub async fn get_server_info(&self, id: i64) -> Result<McpServerInfo, McpServiceError> {
        let server = self.repository.get_by_id(id).await?;
        Ok(self.info_for(server).await)
    }

    /// List all servers with their runtime status.
    pub async fn list_servers_with_status(&self) -> Result<Vec<McpServerInfo>, McpServiceError> {
        let servers = self.repository.list().await?;
        let mut infos = Vec::with_capacity(servers.len());

        for server in servers {
            infos.push(self.info_for(server).await);
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
    ///
    /// For `Lazy` servers that are not yet running, this triggers an automatic start
    /// (deduplicated across concurrent callers via a per-server mutex in the manager).
    /// For `Manual` servers that are not running, this returns a tool-level error so
    /// the model can recover gracefully rather than receiving a transport failure.
    pub async fn call_tool(
        &self,
        server_id: i64,
        tool_name: &str,
        arguments: HashMap<String, serde_json::Value>,
    ) -> Result<McpToolResult, McpServiceError> {
        // Ensure the server is running, honouring its lifecycle policy.
        // Manual servers return a soft tool-level error; all other failures
        // (repo lookup, start failure) propagate as Err so callers see them.
        match self.ensure_started_for_call(server_id).await {
            Ok(()) => {}
            Err(McpServiceError::NotRunning(msg)) => {
                return Ok(McpToolResult {
                    success: false,
                    data: None,
                    error: Some(msg),
                });
            }
            Err(e) => return Err(e),
        }

        self.manager
            .call_tool(server_id, tool_name, arguments)
            .await
            .map_err(|e| McpServiceError::ToolError(e.to_string()))
    }

    /// Ensure a server is running before a tool call, honouring its lifecycle policy.
    ///
    /// - Already running → no-op.
    /// - `Eager` or `Lazy`, not running → attempt to start (via `McpManager::ensure_started`).
    /// - `Manual`, not running → return an error (caller converts to a tool-level error).
    async fn ensure_started_for_call(&self, server_id: i64) -> Result<(), McpServiceError> {
        if self.manager.is_running(server_id).await {
            return Ok(());
        }

        let server = self.repository.get_by_id(server_id).await?;

        match server.lifecycle {
            McpLifecycle::Manual => Err(McpServiceError::NotRunning(format!(
                "Server '{}' is configured as manual and must be started explicitly",
                server.name
            ))),
            McpLifecycle::Eager | McpLifecycle::Lazy => self
                .manager
                .ensure_started(&server)
                .await
                .map(|_| ())
                .map_err(|e| McpServiceError::StartFailed(e.to_string())),
        }
    }

    // =========================================================================
    // Utilities
    // =========================================================================

    /// Stop all running servers.
    pub async fn shutdown(&self) {
        self.manager.stop_all().await;
    }

    /// Test a stored server end to end: resolve its executable, start a
    /// throwaway instance of what is stored, list its tools, then stop it.
    ///
    /// The executable is resolved first, as `start_server` resolves it. A
    /// newly added stdio server has no resolved path yet, and without this
    /// its test fails with "executable path must be absolute" while a start
    /// of the same row succeeds. The resolution's own outcome is not judged
    /// here: a command that cannot be resolved fails the start below, and
    /// that failure is the one reported.
    ///
    /// The throwaway instance is registered under a unique negative id rather
    /// than a fixed one. Real servers use positive ids, so negatives are free;
    /// a *constant* negative was not, because two overlapping tests would both
    /// claim it and the first to finish would stop the other's process — one
    /// user would see a working configuration reported as broken.
    pub async fn test_server(&self, id: i64) -> Result<Vec<McpTool>, McpServiceError> {
        static NEXT_TEST_ID: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(-2);

        let _ = self.ensure_resolved(id).await;

        let mut test_server = self.repository.get_by_id(id).await?;
        let test_id = NEXT_TEST_ID.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        test_server.id = test_id;

        let started = self
            .manager
            .start_server(&test_server)
            .await
            .map_err(|e| McpServiceError::StartFailed(e.to_string()));

        // Stop unconditionally: a start that failed part-way can still have
        // left a process behind, and leaking one per failed test is worse than
        // a redundant stop.
        let stopped = self.manager.stop_server(test_id).await;

        let tools = started?;
        stopped.map_err(|e| McpServiceError::StopFailed(e.to_string()))?;

        Ok(tools)
    }
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
