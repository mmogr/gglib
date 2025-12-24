//! GuiBackend - the unified GUI orchestration facade.
//!
//! This is the main entry point for all GUI operations. Both Tauri
//! commands and Axum handlers delegate to this facade.

use std::net::SocketAddr;

use crate::deps::GuiDeps;
use crate::downloads::DownloadOps;
use crate::error::GuiError;
use crate::mcp::McpOps;
use crate::models::ModelOps;
use crate::proxy::ProxyOps;
use crate::servers::ServerOps;
use crate::settings::SettingsOps;
use crate::types::*;

use gglib_core::ModelFilterOptions;
use gglib_core::download::QueueSnapshot;
use gglib_core::utils::system::SystemMemoryInfo;
use gglib_runtime::proxy::{ProxyConfig, ProxyStatus};

/// Unified GUI backend facade.
///
/// Provides a consistent API for GUI operations, used by both Tauri
/// desktop app and Axum web server. All operations are delegated to
/// specialized ops modules.
///
/// # Construction
///
/// ```ignore
/// let deps = GuiDeps::new(core, downloads, hf, runner, mcp);
/// let backend = GuiBackend::new(deps);
/// ```
pub struct GuiBackend {
    deps: GuiDeps,
}

impl GuiBackend {
    /// Create a new GUI backend with the provided dependencies.
    ///
    /// All dependencies are injected via `GuiDeps` to maintain
    /// adapter neutrality and enable testing.
    pub fn new(deps: GuiDeps) -> Self {
        Self { deps }
    }

    // Accessors for ops modules - created on demand to avoid Arc<&T> issues
    fn model_ops(&self) -> ModelOps<'_> {
        ModelOps::new(&self.deps)
    }

    fn server_ops(&self) -> ServerOps<'_> {
        ServerOps::new(&self.deps)
    }

    fn download_ops(&self) -> DownloadOps<'_> {
        DownloadOps::new(&self.deps)
    }

    fn settings_ops(&self) -> SettingsOps<'_> {
        SettingsOps::new(&self.deps)
    }

    fn mcp_ops(&self) -> McpOps<'_> {
        McpOps::new(&self.deps)
    }

    fn proxy_ops(&self) -> ProxyOps {
        ProxyOps::new(
            self.deps.proxy_supervisor.clone(),
            self.deps.model_repo.clone(),
        )
    }

    // =========================================================================
    // Model operations
    // =========================================================================

    /// List all models with their serving status.
    pub async fn list_models(&self) -> Result<Vec<GuiModel>, GuiError> {
        self.model_ops().list().await
    }

    /// Get a single model by ID.
    pub async fn get_model(&self, id: i64) -> Result<GuiModel, GuiError> {
        self.model_ops().get(id).await
    }

    /// Add a new model from a local file.
    pub async fn add_model(&self, req: AddModelRequest) -> Result<GuiModel, GuiError> {
        self.model_ops().add(req).await
    }

    /// Update an existing model.
    pub async fn update_model(
        &self,
        id: i64,
        req: UpdateModelRequest,
    ) -> Result<GuiModel, GuiError> {
        self.model_ops().update(id, req).await
    }

    /// Remove a model.
    pub async fn remove_model(&self, id: i64, req: RemoveModelRequest) -> Result<String, GuiError> {
        self.model_ops().remove(id, req).await
    }

    /// List all unique tags.
    pub async fn list_tags(&self) -> Result<Vec<String>, GuiError> {
        self.model_ops().list_tags().await
    }

    /// Add a tag to a model.
    pub async fn add_model_tag(&self, id: i64, tag: String) -> Result<(), GuiError> {
        self.model_ops().add_tag(id, tag).await
    }

    /// Remove a tag from a model.
    pub async fn remove_model_tag(&self, id: i64, tag: String) -> Result<(), GuiError> {
        self.model_ops().remove_tag(id, tag).await
    }

    /// Get tags for a model.
    pub async fn get_model_tags(&self, id: i64) -> Result<Vec<String>, GuiError> {
        self.model_ops().get_tags(id).await
    }

    /// Get models with a specific tag.
    pub async fn get_models_by_tag(&self, tag: String) -> Result<Vec<i64>, GuiError> {
        self.model_ops().get_by_tag(tag).await
    }

    /// Get filter options for the model list UI.
    pub async fn get_model_filter_options(&self) -> Result<ModelFilterOptions, GuiError> {
        self.model_ops().get_filter_options().await
    }

    // =========================================================================
    // Server operations
    // =========================================================================

    /// Start a model server.
    pub async fn start_server(
        &self,
        id: i64,
        req: StartServerRequest,
    ) -> Result<StartServerResponse, GuiError> {
        self.server_ops().start(id, req).await
    }

    /// Stop a model server.
    pub async fn stop_server(&self, id: i64) -> Result<String, GuiError> {
        self.server_ops().stop(id).await
    }

    /// Stop all running servers.
    ///
    /// Used during application shutdown to ensure graceful termination.
    pub async fn stop_all_servers(&self) -> Result<(), GuiError> {
        self.server_ops().stop_all().await
    }

    /// List all running servers.
    pub async fn list_servers(&self) -> Vec<ServerInfo> {
        self.server_ops().list_servers().await
    }

    /// Get logs for a specific server port.
    pub fn get_server_logs(&self, port: u16) -> Vec<ServerLogEntry> {
        self.server_ops().get_logs(port)
    }

    /// Subscribe to real-time server log events.
    /// Returns a broadcast receiver that emits ServerLogEntry for all servers.
    pub fn subscribe_server_logs(&self) -> tokio::sync::broadcast::Receiver<ServerLogEntry> {
        self.server_ops().subscribe_logs()
    }

    /// Clear logs for a specific server port.
    pub fn clear_server_logs(&self, port: u16) {
        self.server_ops().clear_logs(port);
    }

    /// Build a server snapshot for event emission.
    ///
    /// Queries all running servers and converts them to `ServerSummary` instances.
    /// This is used to emit the initial snapshot on startup and for health checks.
    pub async fn build_server_snapshot(
        &self,
    ) -> Result<Vec<gglib_core::events::ServerSummary>, GuiError> {
        let servers = self.list_servers().await;
        let mut summaries = Vec::with_capacity(servers.len());

        for server in servers {
            // Query model name for the summary
            match self.deps.models().get_by_id(server.model_id).await {
                Ok(Some(model)) => {
                    summaries.push(gglib_core::events::ServerSummary {
                        id: format!("server-{}", server.model_id),
                        model_id: server.model_id.to_string(),
                        model_name: model.name,
                        port: server.port,
                        healthy: None, // Health status not available in ServerInfo
                    });
                }
                Ok(None) => {
                    // Model not found - still include server but with ID as name
                    summaries.push(gglib_core::events::ServerSummary {
                        id: format!("server-{}", server.model_id),
                        model_id: server.model_id.to_string(),
                        model_name: format!("Model {}", server.model_id),
                        port: server.port,
                        healthy: None,
                    });
                }
                Err(_) => {
                    // Skip servers we can't query (shouldn't happen, but be defensive)
                    continue;
                }
            }
        }

        Ok(summaries)
    }

    /// Emit an initial server snapshot to connected clients.
    ///
    /// This should be called after the GUI backend is fully initialized, typically
    /// with a small delay to ensure all services are ready. The delay matches Tauri's
    /// behavior (200ms) for consistency.
    pub async fn emit_initial_snapshot(&self) {
        // 200ms delay to ensure all services are initialized (matches Tauri behavior)
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        match self.build_server_snapshot().await {
            Ok(snapshot) => {
                self.deps.server_events().snapshot(&snapshot);
            }
            Err(e) => {
                tracing::warn!("Failed to build initial server snapshot: {}", e);
            }
        }
    }

    // =========================================================================
    // Download operations
    // =========================================================================

    /// Queue a download from HuggingFace.
    pub async fn queue_download(
        &self,
        model_id: String,
        quant: Option<String>,
    ) -> Result<(usize, usize), GuiError> {
        self.download_ops().queue_download(model_id, quant).await
    }

    /// Cancel an active or queued download.
    pub async fn cancel_download(&self, model_id: &str) -> Result<(), GuiError> {
        self.download_ops().cancel_download(model_id).await
    }

    /// Get the current download queue snapshot.
    pub async fn get_download_queue(&self) -> QueueSnapshot {
        self.download_ops().get_queue_snapshot().await
    }

    /// Remove a pending download from the queue.
    pub async fn remove_from_download_queue(&self, model_id: &str) -> Result<(), GuiError> {
        self.download_ops().remove_from_queue(model_id).await
    }

    /// Reorder a download in the queue.
    pub async fn reorder_download_queue(
        &self,
        model_id: &str,
        pos: usize,
    ) -> Result<usize, GuiError> {
        self.download_ops().reorder_queue(model_id, pos).await
    }

    /// Reorder the entire download queue.
    pub async fn reorder_download_queue_full(&self, ids: &[String]) -> Result<(), GuiError> {
        self.download_ops().reorder_queue_full(ids).await
    }

    /// Cancel all shards in a shard group.
    pub async fn cancel_shard_group(&self, group_id: &str) -> Result<(), GuiError> {
        self.download_ops().cancel_shard_group(group_id).await
    }

    /// Clear all failed downloads.
    pub async fn clear_failed_downloads(&self) {
        self.download_ops().clear_failed().await
    }

    /// Cancel all downloads (for shutdown).
    pub async fn cancel_all_downloads(&self) {
        self.download_ops().cancel_all().await
    }

    /// Search HuggingFace for models.
    pub async fn browse_hf_models(
        &self,
        req: HfSearchRequest,
    ) -> Result<HfSearchResponse, GuiError> {
        self.download_ops().search_hf_models(req).await
    }

    /// Get available quantizations for a model.
    pub async fn get_model_quantizations(
        &self,
        model_id: &str,
    ) -> Result<HfQuantizationsResponse, GuiError> {
        self.download_ops().get_model_quantizations(model_id).await
    }

    /// Check if a model supports tool calling.
    pub async fn get_hf_tool_support(
        &self,
        model_id: &str,
    ) -> Result<HfToolSupportResponse, GuiError> {
        self.download_ops().get_hf_tool_support(model_id).await
    }

    // =========================================================================
    // Settings operations
    // =========================================================================

    /// Get models directory info for the settings UI.
    pub fn get_models_directory_info(&self) -> Result<ModelsDirectoryInfo, GuiError> {
        self.settings_ops().get_models_directory_info()
    }

    /// Update the models directory.
    pub fn update_models_directory(&self, path: String) -> Result<ModelsDirectoryInfo, GuiError> {
        self.settings_ops().update_models_directory(path)
    }

    /// Get all application settings.
    pub async fn get_settings(&self) -> Result<AppSettings, GuiError> {
        self.settings_ops().get().await
    }

    /// Update application settings.
    pub async fn update_settings(
        &self,
        req: UpdateSettingsRequest,
    ) -> Result<AppSettings, GuiError> {
        self.settings_ops().update(req).await
    }

    /// Get system memory information.
    pub fn get_system_memory(&self) -> Result<Option<SystemMemoryInfo>, GuiError> {
        self.settings_ops().get_system_memory()
    }

    // =========================================================================
    // MCP operations
    // =========================================================================

    /// List all MCP server configurations.
    pub async fn list_mcp_servers(&self) -> Result<Vec<McpServerInfo>, GuiError> {
        self.mcp_ops().list().await
    }

    /// Add a new MCP server configuration.
    pub async fn add_mcp_server(
        &self,
        req: CreateMcpServerRequest,
    ) -> Result<McpServerInfo, GuiError> {
        self.mcp_ops().add(req).await
    }

    /// Update an MCP server configuration.
    pub async fn update_mcp_server(
        &self,
        id: i64,
        req: UpdateMcpServerRequest,
    ) -> Result<McpServerInfo, GuiError> {
        self.mcp_ops().update(id, req).await
    }

    /// Remove an MCP server configuration.
    pub async fn remove_mcp_server(&self, id: i64) -> Result<(), GuiError> {
        self.mcp_ops().remove(id).await
    }

    /// Start an MCP server.
    pub async fn start_mcp_server(&self, id: i64) -> Result<McpServerInfo, GuiError> {
        self.mcp_ops().start(id).await
    }

    /// Stop an MCP server.
    pub async fn stop_mcp_server(&self, id: i64) -> Result<McpServerInfo, GuiError> {
        self.mcp_ops().stop(id).await
    }

    /// List available tools from a running MCP server.
    pub async fn list_mcp_tools(&self, id: i64) -> Result<Vec<McpToolInfo>, GuiError> {
        self.mcp_ops().list_tools(id).await
    }

    /// Call a tool on a running MCP server.
    pub async fn call_mcp_tool(
        &self,
        id: i64,
        req: McpToolCallRequest,
    ) -> Result<McpToolCallResponse, GuiError> {
        self.mcp_ops().call_tool(id, req).await
    }

    /// Resolve MCP server executable path (for diagnostics/auto-fix).
    pub async fn resolve_mcp_server_path(
        &self,
        id: i64,
    ) -> Result<gglib_core::ports::ResolutionStatus, GuiError> {
        self.mcp_ops().resolve_path(id).await
    }

    // =========================================================================
    // Proxy operations
    // =========================================================================

    /// Start the OpenAI-compatible proxy server.
    ///
    /// Creates a SingleSwap ProcessManager internally for model launching.
    /// The llama_base_port is resolved from: override → saved settings → default.
    pub async fn proxy_start(
        &self,
        config: ProxyConfig,
        llama_base_port_override: Option<u16>,
        llama_server_path: String,
    ) -> Result<SocketAddr, GuiError> {
        self.proxy_ops()
            .start(
                self.deps.settings(),
                config,
                llama_base_port_override,
                llama_server_path,
            )
            .await
    }

    /// Stop the proxy server.
    pub async fn proxy_stop(&self) -> Result<(), GuiError> {
        self.proxy_ops().stop().await
    }

    /// Get the current proxy status.
    pub async fn proxy_status(&self) -> ProxyStatus {
        self.proxy_ops().status().await
    }
}
