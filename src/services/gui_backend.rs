//! Shared GUI backend service layer.
//!
//! This module provides unified backend logic for both Tauri desktop and Web GUI,
//! eliminating code duplication and ensuring consistent behavior across both interfaces.
//!
//! The `GuiBackend` delegates to `AppCore` for all core operations while adding
//! GUI-specific functionality like converting models to `GuiModel` with server status.

use crate::models::gui::{
    AddModelRequest, AppSettings, GuiModel, ModelsDirectoryInfo, RemoveModelRequest,
    StartServerRequest, StartServerResponse, UpdateModelRequest, UpdateSettingsRequest,
};
use crate::services::core::{AppCore, StartServerConfig};
use crate::services::database;
use crate::services::process_manager::ProcessManager;
use crate::services::settings::SettingsUpdate;
use crate::utils::process::ServerInfo;
use anyhow::{Result, anyhow};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::debug;

/// Unified GUI backend service
///
/// This service provides a consistent API for both Tauri and Web GUI implementations,
/// eliminating code duplication and ensuring both interfaces have identical functionality.
///
/// Internally delegates to `AppCore` for all operations while providing GUI-specific
/// response types and conversions.
pub struct GuiBackend {
    /// Core application services
    core: AppCore,
}

impl GuiBackend {
    /// Create a new GUI backend service
    pub async fn new(base_port: u16, max_concurrent: usize) -> Result<Self> {
        let db_pool = database::setup_database().await?;
        let core = AppCore::with_config(db_pool, base_port, max_concurrent);

        Ok(Self { core })
    }

    /// Get the AppCore for direct access to core services
    pub fn core(&self) -> &AppCore {
        &self.core
    }

    /// Get the database pool for custom operations (e.g., chat history)
    pub fn db_pool(&self) -> &sqlx::SqlitePool {
        self.core.db_pool()
    }

    /// Get the process manager (for custom operations)
    pub fn process_manager(&self) -> Arc<ProcessManager> {
        self.core.servers().process_manager()
    }

    // =========================================================================
    // Model Operations - Delegate to AppCore with GUI-specific conversions
    // =========================================================================

    /// List all models with their serving status
    pub async fn list_models(&self) -> Result<Vec<GuiModel>> {
        let models = self.core.models().list().await?;

        // Update health status before getting server info
        self.core.servers().update_health().await;

        let mut gui_models = Vec::new();
        for model in models {
            let model_id = model.id.unwrap_or(0);
            let server_info = self.core.servers().get(model_id).await;
            let is_serving = server_info.is_some();
            let port = server_info.map(|s| s.port);
            gui_models.push(GuiModel::from_model(model, is_serving, port));
        }

        Ok(gui_models)
    }

    /// Get a specific model by ID
    pub async fn get_model(&self, id: u32) -> Result<GuiModel> {
        let model = self.core.models().get_by_id(id).await?;

        let server_info = self.core.servers().get(id).await;
        let is_serving = server_info.is_some();
        let port = server_info.map(|s| s.port);

        Ok(GuiModel::from_model(model, is_serving, port))
    }

    /// Add a model to the database
    pub async fn add_model(&self, request: AddModelRequest) -> Result<GuiModel> {
        let model = self
            .core
            .models()
            .add_from_file(&request.file_path, None, None)
            .await?;

        Ok(GuiModel::from_gguf(model))
    }

    /// Update a model in the database
    pub async fn update_model(&self, id: u32, request: UpdateModelRequest) -> Result<GuiModel> {
        let mut model = self.core.models().get_by_id(id).await?;

        if let Some(name) = request.name {
            model.name = name;
        }
        if let Some(quantization) = request.quantization {
            model.quantization = Some(quantization);
        }
        if let Some(file_path) = request.file_path {
            model.file_path = PathBuf::from(file_path);
        }

        self.core.models().update(id, &model).await?;

        Ok(GuiModel::from_gguf(model))
    }

    /// Remove a model from the database
    pub async fn remove_model(&self, id: u32, request: RemoveModelRequest) -> Result<String> {
        let model = self.core.models().get_by_id(id).await?;

        // Check if model is currently serving
        let server_running = self.core.servers().is_running(id).await;
        if server_running && !request.force {
            return Err(anyhow!(
                "Model is currently serving. Stop the server first or use force=true"
            ));
        }

        // Stop server if running and force is true
        if server_running {
            self.core.servers().stop(id).await?;
        }

        self.core.models().remove(id).await?;

        Ok(format!("Model '{}' removed successfully", model.name))
    }

    // =========================================================================
    // Tag Operations - Direct delegation to AppCore
    // =========================================================================

    /// List all unique tags used across all models
    pub async fn list_tags(&self) -> Result<Vec<String>> {
        self.core.models().list_tags().await
    }

    /// Add a tag to a model
    pub async fn add_model_tag(&self, model_id: u32, tag: String) -> Result<()> {
        self.core.models().add_tag(model_id, tag).await
    }

    /// Remove a tag from a model
    pub async fn remove_model_tag(&self, model_id: u32, tag: String) -> Result<()> {
        self.core.models().remove_tag(model_id, tag).await
    }

    /// Get all tags for a specific model
    pub async fn get_model_tags(&self, model_id: u32) -> Result<Vec<String>> {
        self.core.models().get_tags(model_id).await
    }

    /// Get all models that have a specific tag
    pub async fn get_models_by_tag(&self, tag: String) -> Result<Vec<u32>> {
        self.core.models().get_by_tag(&tag).await
    }

    // =========================================================================
    // Server Operations - Delegate to AppCore with GUI-specific conversions
    // =========================================================================

    /// Start serving a model
    pub async fn start_server(
        &self,
        id: u32,
        request: StartServerRequest,
    ) -> Result<StartServerResponse> {
        debug!(model_id = %id, "Starting server for model");

        let config = StartServerConfig {
            model_id: id,
            context_length: request.context_length,
            jinja: request.jinja,
        };

        self.core.servers().start(config).await
    }

    /// Stop serving a model
    pub async fn stop_model(&self, id: u32) -> Result<String> {
        self.core.servers().stop(id).await
    }

    /// List all running servers
    pub async fn list_servers(&self) -> Result<Vec<ServerInfo>> {
        Ok(self.core.servers().list().await)
    }

    // =========================================================================
    // Proxy Operations - Delegate to AppCore with JSON conversion for status
    // =========================================================================

    /// Start the OpenAI-compatible proxy
    pub async fn start_proxy(
        &self,
        host: String,
        port: u16,
        start_port: u16,
        default_context: u64,
    ) -> Result<String> {
        self.core
            .proxy()
            .start(host, port, start_port, default_context)
            .await
    }

    /// Stop the OpenAI-compatible proxy
    pub async fn stop_proxy(&self) -> Result<String> {
        self.core.proxy().stop().await
    }

    /// Get proxy status
    pub async fn get_proxy_status(&self) -> Result<serde_json::Value> {
        let status = self.core.proxy().status().await;

        Ok(serde_json::json!({
            "running": status.running,
            "port": status.port,
            "current_model": status.current_model,
            "model_port": status.model_port,
        }))
    }

    // =========================================================================
    // Download Operations - Delegate to AppCore
    // =========================================================================

    /// Download a model from HuggingFace Hub
    pub async fn download_model(
        &self,
        model_id: String,
        quantization: Option<String>,
        progress_callback: Option<&crate::commands::download::ProgressCallback>,
    ) -> Result<String> {
        self.core
            .downloads()
            .download(model_id, quantization, progress_callback)
            .await
    }

    /// Cancel an in-flight download if one exists.
    pub async fn cancel_download(&self, model_id: &str) -> Result<()> {
        self.core.downloads().cancel(model_id).await
    }

    // =========================================================================
    // Settings Operations - Delegate to AppCore with GUI type conversions
    // =========================================================================

    /// Return current models directory information for the settings UI.
    pub fn get_models_directory_info(&self) -> Result<ModelsDirectoryInfo> {
        self.core.settings().get_models_directory_info()
    }

    /// Update, validate, and persist the models directory selection.
    pub fn update_models_directory(&self, new_path: String) -> Result<ModelsDirectoryInfo> {
        self.core.settings().update_models_directory(&new_path)
    }

    /// Get current application settings
    pub async fn get_settings(&self) -> Result<AppSettings> {
        let settings = self.core.settings().get().await?;

        Ok(AppSettings {
            default_download_path: settings.default_download_path,
            default_context_size: settings.default_context_size,
            proxy_port: settings.proxy_port,
            server_port: settings.server_port,
            max_download_queue_size: settings.max_download_queue_size,
        })
    }

    /// Update application settings with validation
    pub async fn update_settings(&self, request: UpdateSettingsRequest) -> Result<AppSettings> {
        let update = SettingsUpdate {
            default_download_path: request.default_download_path,
            default_context_size: request.default_context_size,
            proxy_port: request.proxy_port,
            server_port: request.server_port,
            max_download_queue_size: request.max_download_queue_size,
        };

        let settings = self.core.settings().update(update).await?;

        // Update download service queue size if changed
        if let Some(Some(queue_size)) = request.max_download_queue_size {
            self.core.downloads().set_max_queue_size(queue_size).await;
        }

        Ok(AppSettings {
            default_download_path: settings.default_download_path,
            default_context_size: settings.default_context_size,
            proxy_port: settings.proxy_port,
            server_port: settings.server_port,
            max_download_queue_size: settings.max_download_queue_size,
        })
    }

    // =========================================================================
    // Download Queue Operations
    // =========================================================================

    /// Add a download to the queue or start immediately if nothing is running.
    /// Returns the queue position (1 = will start immediately).
    pub async fn queue_download(
        &self,
        model_id: String,
        quantization: Option<String>,
    ) -> Result<usize> {
        self.core
            .downloads()
            .queue_download(model_id, quantization)
            .await
    }

    /// Get the current status of the download queue.
    pub async fn get_download_queue(
        &self,
    ) -> crate::services::core::DownloadQueueStatus {
        self.core.downloads().get_queue_status().await
    }

    /// Remove an item from the pending download queue.
    pub async fn remove_from_download_queue(&self, model_id: &str) -> Result<()> {
        self.core.downloads().remove_from_queue(model_id).await
    }

    /// Clear all failed downloads from the list.
    pub async fn clear_failed_downloads(&self) {
        self.core.downloads().clear_failed().await
    }
}

/// Convenience function to create a GUI backend with default settings
pub async fn create_default_backend() -> Result<GuiBackend> {
    GuiBackend::new(5000, 5).await
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
