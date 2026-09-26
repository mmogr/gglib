//! Settings operations for GUI backend.

use std::sync::Arc;

use gglib_core::SettingsUpdate;
use gglib_core::paths::{ModelsDirSource, resolve_models_dir};
use gglib_core::ports::{DownloadManagerPort, SystemProbePort};
use gglib_core::services::AppCore;
use gglib_core::utils::system::SystemMemoryInfo;

use crate::error::GuiError;
use crate::types::{AppSettings, ModelsDirectoryInfo, UpdateSettingsRequest};

/// Format `ModelsDirSource` for display.
fn format_source(source: ModelsDirSource) -> &'static str {
    match source {
        ModelsDirSource::Explicit => "explicit",
        ModelsDirSource::EnvVar => "environment",
        ModelsDirSource::Default => "default",
    }
}

/// Dependencies for settings operations.
pub struct SettingsDeps {
    pub core: Arc<AppCore>,
    pub system_probe: Arc<dyn SystemProbePort>,
    pub downloads: Arc<dyn DownloadManagerPort>,
}

/// Settings operations handler.
pub struct SettingsOps {
    deps: SettingsDeps,
}

impl SettingsOps {
    pub fn new(deps: SettingsDeps) -> Self {
        Self { deps }
    }

    /// Return current models directory information for the settings UI.
    pub fn get_models_directory_info(&self) -> Result<ModelsDirectoryInfo, GuiError> {
        let resolution = resolve_models_dir(None)
            .map_err(|e| GuiError::Internal(format!("Failed to resolve models dir: {e}")))?;

        let default_path = dirs::data_dir()
            .map_or_else(
                || std::path::PathBuf::from("models"),
                |p| p.join("gglib").join("models"),
            )
            .to_string_lossy()
            .to_string();

        let exists = resolution.path.exists();
        let writable = exists
            && std::fs::metadata(&resolution.path).is_ok_and(|m| !m.permissions().readonly());

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
            .map_or_else(
                || std::path::PathBuf::from("models"),
                |p| p.join("gglib").join("models"),
            )
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
            .core
            .settings()
            .get()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to get settings: {e}")))?;

        Ok(settings.into())
    }

    /// Update application settings with validation.
    pub async fn update(&self, request: UpdateSettingsRequest) -> Result<AppSettings, GuiError> {
        let update: SettingsUpdate = request.clone().into();

        let settings = self
            .deps
            .core
            .settings()
            .update(update)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to update settings: {e}")))?;

        if let Some(Some(queue_size)) = request.max_download_queue_size {
            let _ = self.deps.downloads.set_max_queue_size(queue_size).await;
        }

        Ok(settings.into())
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

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
