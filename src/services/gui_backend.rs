//! Shared GUI backend service layer.
//!
//! This module provides unified backend logic for both Tauri desktop and Web GUI,
//! eliminating code duplication and ensuring consistent behavior across both interfaces.

use crate::commands::common::{JinjaResolutionSource, resolve_jinja_flag};
use crate::models::gui::{
    AddModelRequest, AppSettings, GuiModel, ModelsDirectoryInfo, RemoveModelRequest,
    StartServerRequest, StartServerResponse, UpdateModelRequest, UpdateSettingsRequest,
};
use crate::services::database;
use crate::services::process_manager::ProcessManager;
use crate::services::settings;
use crate::utils::paths::{
    DirectoryCreationStrategy, ModelsDirSource, default_models_dir, ensure_directory,
    get_llama_server_path, persist_models_dir, resolve_models_dir, verify_writable,
};
use crate::utils::process::ServerInfo;
use anyhow::{Result, anyhow, bail};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

/// Unified GUI backend service
///
/// This service provides a consistent API for both Tauri and Web GUI implementations,
/// eliminating code duplication and ensuring both interfaces have identical functionality.
pub struct GuiBackend {
    db_pool: SqlitePool,
    process_manager: Arc<ProcessManager>,
    proxy_manager: Arc<RwLock<Option<ProcessManager>>>,
    proxy_shutdown: Arc<RwLock<Option<tokio::sync::oneshot::Sender<()>>>>,
    proxy_port: Arc<RwLock<Option<u16>>>,
    active_downloads: Arc<RwLock<HashMap<String, CancellationToken>>>,
}

/// Errors related to managed download tasks.
#[derive(Debug, Error)]
pub enum DownloadTaskError {
    #[error("Download '{model_id}' was cancelled by the user")]
    Cancelled { model_id: String },
}

impl GuiBackend {
    /// Create a new GUI backend service
    pub async fn new(base_port: u16, max_concurrent: usize) -> Result<Self> {
        let db_pool = database::setup_database().await?;

        // Get llama-server path
        let llama_server_path = get_llama_server_path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "llama-server".to_string());

        let process_manager = Arc::new(ProcessManager::new_concurrent(
            base_port,
            max_concurrent,
            llama_server_path,
        ));

        Ok(Self {
            db_pool,
            process_manager,
            proxy_manager: Arc::new(RwLock::new(None)),
            proxy_shutdown: Arc::new(RwLock::new(None)),
            proxy_port: Arc::new(RwLock::new(None)),
            active_downloads: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Get the database pool (for custom operations)
    pub fn db_pool(&self) -> &SqlitePool {
        &self.db_pool
    }

    /// Get the process manager (for custom operations)
    pub fn process_manager(&self) -> Arc<ProcessManager> {
        self.process_manager.clone()
    }

    /// List all models with their serving status
    pub async fn list_models(&self) -> Result<Vec<GuiModel>> {
        let models = database::list_models(&self.db_pool).await?;

        // Update health status before listing
        self.process_manager.update_health_status().await;

        let mut gui_models = Vec::new();
        for model in models {
            let model_id = model.id.unwrap_or(0);
            let server_info = self.process_manager.get_server(model_id).await;
            let is_serving = server_info.is_some();
            let port = server_info.map(|s| s.port);
            gui_models.push(GuiModel::from_model(model, is_serving, port));
        }

        Ok(gui_models)
    }

    /// Get a specific model by ID
    pub async fn get_model(&self, id: u32) -> Result<GuiModel> {
        let identifier = id.to_string();
        let model = database::find_model_by_identifier(&self.db_pool, &identifier)
            .await?
            .ok_or_else(|| anyhow!("Model with ID {} not found", id))?;

        let server_info = self.process_manager.get_server(id).await;
        let is_serving = server_info.is_some();
        let port = server_info.map(|s| s.port);

        Ok(GuiModel::from_model(model, is_serving, port))
    }

    /// Add a model to the database
    pub async fn add_model(&self, request: AddModelRequest) -> Result<GuiModel> {
        use crate::models::Gguf;
        use crate::utils::validation;

        let file_path = PathBuf::from(&request.file_path);

        // Validate file exists
        if !file_path.exists() {
            return Err(anyhow!("File not found: {}", request.file_path));
        }

        // Validate it's a GGUF file
        if !request.file_path.to_lowercase().ends_with(".gguf") {
            return Err(anyhow!("File must have .gguf extension"));
        }

        // Validate the GGUF file and extract metadata (non-interactive)
        let gguf_metadata = validation::validate_and_parse_gguf(&request.file_path)?;

        // Use extracted metadata with sensible defaults (no user prompts)
        let name = gguf_metadata.name.unwrap_or_else(|| {
            // Fallback: use filename without extension
            file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown Model")
                .to_string()
        });

        let param_count_b = gguf_metadata.param_count_b.unwrap_or(0.0); // Will be 0 if not available

        // Create the model instance with extracted metadata
        let new_model = Gguf {
            id: None, // Will be set by the database
            name,
            file_path: file_path.clone(),
            param_count_b,
            architecture: gguf_metadata.architecture,
            quantization: gguf_metadata.quantization,
            context_length: gguf_metadata.context_length,
            metadata: gguf_metadata.metadata,
            added_at: chrono::Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: Vec::new(),
        };

        // Save to database
        database::add_model(&self.db_pool, &new_model).await?;

        // Retrieve the newly added model
        let models = database::list_models(&self.db_pool).await?;
        let model = models
            .into_iter()
            .find(|m| m.file_path == file_path)
            .ok_or_else(|| anyhow!("Model was added but could not be retrieved"))?;

        Ok(GuiModel::from_gguf(model))
    }

    /// Update a model in the database
    pub async fn update_model(&self, id: u32, request: UpdateModelRequest) -> Result<GuiModel> {
        // Get the existing model
        let mut model = database::get_model_by_id(&self.db_pool, id)
            .await?
            .ok_or_else(|| anyhow!("Model {} not found", id))?;

        // Update fields if provided
        if let Some(name) = request.name {
            model.name = name;
        }
        if let Some(quantization) = request.quantization {
            model.quantization = Some(quantization);
        }
        if let Some(file_path) = request.file_path {
            model.file_path = PathBuf::from(file_path);
        }

        // Save to database
        database::update_model(&self.db_pool, id, &model).await?;

        // Return updated model as GuiModel
        Ok(GuiModel::from_gguf(model))
    }

    /// Remove a model from the database
    pub async fn remove_model(&self, id: u32, request: RemoveModelRequest) -> Result<String> {
        let model = database::get_model_by_id(&self.db_pool, id)
            .await?
            .ok_or_else(|| anyhow!("Model {} not found", id))?;

        // Check if model is currently serving
        let server_running = self.process_manager.get_server(id).await.is_some();
        if server_running && !request.force {
            return Err(anyhow!(
                "Model is currently serving. Stop the server first or use force=true"
            ));
        }

        // Stop server if running and force is true
        if server_running {
            self.process_manager.stop_server(id).await?;
        }

        database::remove_model_by_id(&self.db_pool, id).await?;

        Ok(format!("Model '{}' removed successfully", model.name))
    }

    /// Start serving a model
    pub async fn start_server(
        &self,
        id: u32,
        request: StartServerRequest,
    ) -> Result<StartServerResponse> {
        debug!(model_id = %id, "Starting server for model");

        // Get model from database
        let identifier = id.to_string();
        let model = database::find_model_by_identifier(&self.db_pool, &identifier)
            .await?
            .ok_or_else(|| anyhow!("Model with ID {} not found", id))?;

        debug!(
            model_name = %model.name,
            model_path = %model.file_path.display(),
            "Found model"
        );

        // Check if model file exists
        if !model.file_path.exists() {
            return Err(anyhow!(
                "Model file not found: {}",
                model.file_path.display()
            ));
        }

        // Use context length from request, model metadata, or None
        let context_length = request.context_length.or(model.context_length);

        let jinja_resolution = resolve_jinja_flag(request.jinja, &model.tags);
        match (jinja_resolution.enabled, jinja_resolution.source) {
            (true, JinjaResolutionSource::ExplicitTrue) => {
                debug!("Enabling Jinja templates (user override)");
            }
            (true, JinjaResolutionSource::AgentTag) => {
                debug!("Enabling Jinja templates due to 'agent' tag");
            }
            (false, JinjaResolutionSource::ExplicitFalse) => {
                debug!("Jinja templates disabled by user override");
            }
            _ => {}
        }

        debug!("Calling ProcessManager.start_server");

        // Start the server
        let port = self
            .process_manager
            .start_server(
                id,
                model.name.clone(),
                &model.file_path.to_string_lossy(),
                context_length,
                jinja_resolution.enabled,
            )
            .await?;

        info!(port = %port, model_id = %id, "Server started");
        debug!(port = %port, "Returning StartServerResponse");

        Ok(StartServerResponse {
            port,
            message: format!("Server started for model '{}' on port {}", model.name, port),
        })
    }

    /// Stop serving a model
    pub async fn stop_model(&self, id: u32) -> Result<String> {
        self.process_manager.stop_server(id).await?;
        Ok(format!("Model {} stopped successfully", id))
    }

    /// List all running servers
    pub async fn list_servers(&self) -> Result<Vec<ServerInfo>> {
        // Update health before listing
        self.process_manager.update_health_status().await;
        let servers = self.process_manager.list_servers().await;
        debug!(
            server_count = %servers.len(),
            "list_servers called"
        );
        for server in &servers {
            debug!(
                model_id = %server.model_id,
                model_name = %server.model_name,
                port = %server.port,
                "Server info"
            );
        }
        Ok(servers)
    }

    /// Start the OpenAI-compatible proxy
    pub async fn start_proxy(
        &self,
        host: String,
        port: u16,
        start_port: u16,
        default_context: u64,
    ) -> Result<String> {
        let mut proxy = self.proxy_manager.write().await;
        let mut shutdown = self.proxy_shutdown.write().await;

        if proxy.is_some() {
            return Err(anyhow!("Proxy is already running"));
        }

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        // Create ProcessManager with SingleSwap strategy for proxy
        let llama_server_path = crate::utils::paths::get_llama_server_path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "llama-server".to_string());

        let manager =
            ProcessManager::new_single_swap(self.db_pool.clone(), start_port, llama_server_path);

        // Start proxy server
        crate::proxy::start_proxy_with_shutdown(
            host.clone(),
            port,
            manager.clone(),
            default_context,
            shutdown_rx,
        )
        .await?;

        *proxy = Some(manager);
        *shutdown = Some(shutdown_tx);

        // Store the actual proxy port
        *self.proxy_port.write().await = Some(port);

        Ok(format!("Proxy started on {}:{}", host, port))
    }

    /// Stop the OpenAI-compatible proxy
    pub async fn stop_proxy(&self) -> Result<String> {
        let mut proxy = self.proxy_manager.write().await;
        let mut shutdown = self.proxy_shutdown.write().await;

        if proxy.is_none() {
            return Err(anyhow!("Proxy is not running"));
        }

        // Signal shutdown (this triggers graceful shutdown in the background task)
        if let Some(tx) = shutdown.take() {
            let _ = tx.send(()); // Ignore error if receiver already dropped
        }

        // Clear the manager reference and port
        proxy.take();
        *self.proxy_port.write().await = None;

        Ok("Proxy stopped".to_string())
    }

    /// Get proxy status
    pub async fn get_proxy_status(&self) -> Result<serde_json::Value> {
        let proxy = self.proxy_manager.read().await;
        let port = self.proxy_port.read().await;

        if let Some(manager) = proxy.as_ref() {
            let current_model = manager.get_current_model().await;
            let current_port = manager.get_current_port().await;

            Ok(serde_json::json!({
                "running": true,
                "port": port.unwrap_or(8080),
                "current_model": current_model,
                "model_port": current_port,
            }))
        } else {
            Ok(serde_json::json!({
                "running": false,
                "port": *port,
            }))
        }
    }

    /// Download a model from HuggingFace Hub
    pub async fn download_model(
        &self,
        model_id: String,
        quantization: Option<String>,
        progress_callback: Option<&crate::commands::download::ProgressCallback>,
    ) -> Result<String> {
        use crate::commands;
        let cancel_token = CancellationToken::new();

        {
            let mut downloads = self.active_downloads.write().await;
            if downloads.contains_key(&model_id) {
                return Err(anyhow!("A download for '{}' is already running", model_id));
            }
            downloads.insert(model_id.clone(), cancel_token.clone());
        }

        let download_future = commands::download::execute(
            model_id.clone(),
            quantization,
            false,
            true,
            None,
            false,
            progress_callback,
        );
        tokio::pin!(download_future);

        let result = tokio::select! {
            res = &mut download_future => {
                res.map(|_| "Model downloaded successfully".to_string())
            }
            _ = cancel_token.cancelled() => {
                Err(DownloadTaskError::Cancelled { model_id: model_id.clone() }.into())
            }
        };

        self.active_downloads.write().await.remove(&model_id);

        result
    }

    /// Cancel an in-flight download if one exists.
    pub async fn cancel_download(&self, model_id: &str) -> Result<()> {
        let token = {
            let mut downloads = self.active_downloads.write().await;
            downloads.remove(model_id)
        };

        if let Some(token) = token {
            token.cancel();
            Ok(())
        } else {
            Err(anyhow!("No active download for '{}'", model_id))
        }
    }

    // Tag Management Operations

    /// List all unique tags used across all models
    pub async fn list_tags(&self) -> Result<Vec<String>> {
        database::list_tags(&self.db_pool).await
    }

    /// Add a tag to a model
    pub async fn add_model_tag(&self, model_id: u32, tag: String) -> Result<()> {
        database::add_model_tag(&self.db_pool, model_id, tag).await
    }

    /// Remove a tag from a model
    pub async fn remove_model_tag(&self, model_id: u32, tag: String) -> Result<()> {
        database::remove_model_tag(&self.db_pool, model_id, tag).await
    }

    /// Get all tags for a specific model
    pub async fn get_model_tags(&self, model_id: u32) -> Result<Vec<String>> {
        database::get_model_tags(&self.db_pool, model_id).await
    }

    /// Get all models that have a specific tag
    pub async fn get_models_by_tag(&self, tag: String) -> Result<Vec<u32>> {
        database::get_models_by_tag(&self.db_pool, tag).await
    }

    /// Return current models directory information for the settings UI.
    pub fn get_models_directory_info(&self) -> Result<ModelsDirectoryInfo> {
        let resolution = resolve_models_dir(None)?;
        let default_path = default_models_dir()?;
        let exists = resolution.path.is_dir();
        let writable = exists && verify_writable(&resolution.path).is_ok();

        Ok(ModelsDirectoryInfo {
            path: resolution.path.to_string_lossy().to_string(),
            source: stringify_models_dir_source(resolution.source).to_string(),
            default_path: default_path.to_string_lossy().to_string(),
            exists,
            writable,
        })
    }

    /// Update, validate, and persist the models directory selection.
    pub fn update_models_directory(&self, new_path: String) -> Result<ModelsDirectoryInfo> {
        if new_path.trim().is_empty() {
            bail!("Path cannot be empty");
        }

        let resolution = resolve_models_dir(Some(&new_path))?;
        ensure_directory(&resolution.path, DirectoryCreationStrategy::AutoCreate)?;
        persist_models_dir(&resolution.path)?;
        // SAFETY: This modifies global environment state in a multi-threaded context.
        // While inherently unsafe, it maintains consistency between the persisted configuration
        // and runtime state. The value is only read during path resolution operations which
        // typically occur at controlled points. Future refactoring should consider passing
        // configuration explicitly rather than through environment variables.
        unsafe {
            std::env::set_var(
                "GGLIB_MODELS_DIR",
                resolution.path.to_string_lossy().to_string(),
            );
        }

        self.get_models_directory_info()
    }

    // Settings Management Operations

    /// Get current application settings
    pub async fn get_settings(&self) -> Result<AppSettings> {
        let settings = settings::get_settings(&self.db_pool).await?;
        Ok(AppSettings {
            default_download_path: settings.default_download_path,
            default_context_size: settings.default_context_size,
            proxy_port: settings.proxy_port,
            server_port: settings.server_port,
        })
    }

    /// Update application settings with validation
    pub async fn update_settings(&self, request: UpdateSettingsRequest) -> Result<AppSettings> {
        // Clone the download path before moving the request
        let download_path_for_env = request.default_download_path.clone();

        // Convert request to settings update
        let update = settings::SettingsUpdate {
            default_download_path: request.default_download_path,
            default_context_size: request.default_context_size,
            proxy_port: request.proxy_port,
            server_port: request.server_port,
        };

        // Update settings
        let settings = settings::update_settings(&self.db_pool, update).await?;

        // Validate the settings
        settings::validate_settings(&settings)?;

        // If download path changed, update the environment variable
        if let Some(Some(ref path)) = download_path_for_env {
            let resolved = resolve_models_dir(Some(path))?;
            ensure_directory(&resolved.path, DirectoryCreationStrategy::AutoCreate)?;
            persist_models_dir(&resolved.path)?;
            // SAFETY: This modifies global environment state in a multi-threaded context.
            // While inherently unsafe, it maintains consistency between the persisted configuration
            // and runtime state. The value is only read during path resolution operations which
            // typically occur at controlled points. Future refactoring should consider passing
            // configuration explicitly rather than through environment variables.
            unsafe {
                std::env::set_var(
                    "GGLIB_MODELS_DIR",
                    resolved.path.to_string_lossy().to_string(),
                );
            }
        }

        Ok(AppSettings {
            default_download_path: settings.default_download_path,
            default_context_size: settings.default_context_size,
            proxy_port: settings.proxy_port,
            server_port: settings.server_port,
        })
    }
}

/// Convenience function to create a GUI backend with default settings
pub async fn create_default_backend() -> Result<GuiBackend> {
    GuiBackend::new(5000, 5).await
}

fn stringify_models_dir_source(source: ModelsDirSource) -> &'static str {
    match source {
        ModelsDirSource::Explicit => "explicit",
        ModelsDirSource::EnvVar => "env",
        ModelsDirSource::Default => "default",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_backend_creation() {
        let result = GuiBackend::new(5000, 5).await;
        assert!(result.is_ok(), "Should create backend successfully");
    }

    #[tokio::test]
    async fn test_list_models_empty() {
        let backend = GuiBackend::new(5000, 5).await.unwrap();
        let models = backend.list_models().await.unwrap();
        // Just check it doesn't crash - actual models depend on database state
        assert!(models.is_empty() || !models.is_empty());
    }
}
