//! Settings operations for GUI backend.

use gglib_core::SettingsUpdate;
use gglib_core::paths::{ModelsDirSource, resolve_models_dir};
use gglib_core::utils::system::SystemMemoryInfo;

use crate::deps::GuiDeps;
use crate::error::GuiError;
use crate::types::{AppSettings, ModelsDirectoryInfo, UpdateSettingsRequest};

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
            max_tool_iterations: settings.max_tool_iterations,
            max_stagnation_steps: settings.max_stagnation_steps,
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
            max_tool_iterations: request.max_tool_iterations,
            max_stagnation_steps: request.max_stagnation_steps,
            default_model_id: None, // Not exposed in GUI settings yet
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
            max_tool_iterations: settings.max_tool_iterations,
            max_stagnation_steps: settings.max_stagnation_steps,
        })
    }

    /// Get system memory information.
    ///
    /// Returns None if memory information is unavailable (probe failed, too small, etc.).
    pub fn get_system_memory(&self) -> Result<Option<SystemMemoryInfo>, GuiError> {
        let mem_info = self.deps.system_probe.get_system_memory_info();

        // Treat suspiciously small values as invalid (< 256MB suggests probe failure)
        const MIN_VALID_MEMORY: u64 = 256 * 1024 * 1024; // 256 MB

        if mem_info.total_ram_bytes < MIN_VALID_MEMORY {
            tracing::warn!(
                "System memory probe returned suspiciously low value: {} bytes. \
                 Treating as unavailable.",
                mem_info.total_ram_bytes
            );
            return Ok(None);
        }

        Ok(Some(mem_info))
    }
}
