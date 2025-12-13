//! Settings operations for GUI backend.

use gglib_core::SettingsUpdate;
use gglib_core::paths::{ModelsDirSource, resolve_models_dir};

use crate::deps::GuiDeps;
use crate::error::GuiError;
use crate::types::{AppSettings, ModelsDirectoryInfo, SystemMemoryInfo, UpdateSettingsRequest};

/// Format ModelsDirSource for display.
fn format_source(source: ModelsDirSource) -> &'static str {
    match source {
        ModelsDirSource::Explicit => "explicit",
        ModelsDirSource::EnvVar => "environment",
        ModelsDirSource::Default => "default",
    }
}

/// Settings operations handler.
pub struct SettingsOps<'a> {
    deps: &'a GuiDeps,
}

impl<'a> SettingsOps<'a> {
    pub fn new(deps: &'a GuiDeps) -> Self {
        Self { deps }
    }

    /// Return current models directory information for the settings UI.
    pub fn get_models_directory_info(&self) -> Result<ModelsDirectoryInfo, GuiError> {
        let resolution = resolve_models_dir(None)
            .map_err(|e| GuiError::Internal(format!("Failed to resolve models dir: {e}")))?;

        let default_path = dirs::data_dir()
            .map(|p| p.join("gglib").join("models"))
            .unwrap_or_else(|| std::path::PathBuf::from("models"))
            .to_string_lossy()
            .to_string();

        let exists = resolution.path.exists();
        let writable = exists
            && std::fs::metadata(&resolution.path)
                .map(|m| !m.permissions().readonly())
                .unwrap_or(false);

        Ok(ModelsDirectoryInfo {
            path: resolution.path.to_string_lossy().to_string(),
            source: format_source(resolution.source).to_string(),
            default_path,
            exists,
            writable,
        })
    }

    /// Update the models directory.
    pub fn update_models_directory(
        &self,
        new_path: String,
    ) -> Result<ModelsDirectoryInfo, GuiError> {
        let path = std::path::PathBuf::from(&new_path);
        if !path.exists() {
            std::fs::create_dir_all(&path).map_err(|e| {
                GuiError::ValidationFailed(format!("Failed to create directory: {e}"))
            })?;
        }

        let resolution = resolve_models_dir(Some(new_path.as_str()))
            .map_err(|e| GuiError::Internal(format!("Failed to resolve models dir: {e}")))?;

        let default_path = dirs::data_dir()
            .map(|p| p.join("gglib").join("models"))
            .unwrap_or_else(|| std::path::PathBuf::from("models"))
            .to_string_lossy()
            .to_string();

        Ok(ModelsDirectoryInfo {
            path: resolution.path.to_string_lossy().to_string(),
            source: "user".to_string(),
            default_path,
            exists: resolution.path.exists(),
            writable: true,
        })
    }

    /// Get current application settings.
    pub async fn get(&self) -> Result<AppSettings, GuiError> {
        let settings = self
            .deps
            .settings()
            .get()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to get settings: {e}")))?;

        Ok(AppSettings {
            default_download_path: settings.default_download_path,
            default_context_size: settings.default_context_size,
            proxy_port: settings.proxy_port,
            llama_base_port: settings.llama_base_port,
            max_download_queue_size: settings.max_download_queue_size,
            show_memory_fit_indicators: settings.show_memory_fit_indicators,
        })
    }

    /// Update application settings with validation.
    pub async fn update(&self, request: UpdateSettingsRequest) -> Result<AppSettings, GuiError> {
        let update = SettingsUpdate {
            default_download_path: request.default_download_path,
            default_context_size: request.default_context_size,
            proxy_port: request.proxy_port,
            llama_base_port: request.llama_base_port,
            max_download_queue_size: request.max_download_queue_size,
            show_memory_fit_indicators: request.show_memory_fit_indicators,
        };

        let settings = self
            .deps
            .settings()
            .update(update)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to update settings: {e}")))?;

        if let Some(Some(queue_size)) = request.max_download_queue_size {
            let _ = self.deps.downloads.set_max_queue_size(queue_size).await;
        }

        Ok(AppSettings {
            default_download_path: settings.default_download_path,
            default_context_size: settings.default_context_size,
            proxy_port: settings.proxy_port,
            llama_base_port: settings.llama_base_port,
            max_download_queue_size: settings.max_download_queue_size,
            show_memory_fit_indicators: settings.show_memory_fit_indicators,
        })
    }

    /// Get system memory information.
    pub fn get_system_memory(&self) -> Result<SystemMemoryInfo, GuiError> {
        // Use sysinfo or similar to get memory info
        // For now, return placeholder values
        Ok(SystemMemoryInfo {
            total_bytes: 0,
            available_bytes: 0,
            used_bytes: 0,
        })
    }
}
