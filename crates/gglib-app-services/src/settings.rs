//! Settings operations for GUI backend.

use std::sync::Arc;

use gglib_core::SettingsUpdate;
use gglib_core::paths::{
    DirectoryCreationStrategy, ModelsDirSource, PathError, default_models_dir, resolve_models_dir,
    set_models_dir,
};
use gglib_core::ports::{DownloadManagerPort, SystemProbePort};
use gglib_core::services::AppCore;
use gglib_core::utils::system::SystemMemoryInfo;

use crate::error::GuiError;
use crate::types::{AppSettings, ModelsDirectoryInfo, UpdateSettingsRequest};

/// A probe that reports less RAM than this has not read it.
const MIN_VALID_MEMORY: u64 = 256 * 1024 * 1024;

/// Format `ModelsDirSource` for display.
fn format_source(source: ModelsDirSource) -> &'static str {
    match source {
        ModelsDirSource::Explicit => "explicit",
        ModelsDirSource::EnvVar => "environment",
        ModelsDirSource::Default => "default",
    }
}

/// The models directory as it resolves now: where it is, how it was chosen,
/// the default it would otherwise be, and whether it is there to write to.
///
/// The one description of the directory, for the settings page and the setup
/// status alike. The default is core's own, and is blank where there is no
/// home directory to put one under.
pub(crate) fn models_directory_info() -> Result<ModelsDirectoryInfo, PathError> {
    let resolution = resolve_models_dir(None)?;
    let exists = resolution.path.exists();
    let writable =
        exists && std::fs::metadata(&resolution.path).is_ok_and(|m| !m.permissions().readonly());

    Ok(ModelsDirectoryInfo {
        path: resolution.path.to_string_lossy().to_string(),
        source: format_source(resolution.source).to_string(),
        default_path: default_models_dir()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default(),
        exists,
        writable,
    })
}

/// The probe's memory reading, or `None` for a figure too small to be one.
///
/// The one rule for every route that reports memory.
pub(crate) fn system_memory(probe: &dyn SystemProbePort) -> Option<SystemMemoryInfo> {
    let mem_info = probe.get_system_memory_info();
    if mem_info.total_ram_bytes < MIN_VALID_MEMORY {
        tracing::warn!(
            "System memory probe returned suspiciously low value: {} bytes. \
             Treating as unavailable.",
            mem_info.total_ram_bytes
        );
        return None;
    }
    Some(mem_info)
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
        models_directory_info()
            .map_err(|e| GuiError::Internal(format!("Failed to resolve models dir: {e}")))
    }

    /// Make `new_path` the models directory, creating it if it is not there,
    /// and return the directory as it resolves afterwards.
    ///
    /// The operation `gglib config models-dir set` runs. What it stores is
    /// read by every later run, and by this one unless its environment names
    /// a directory of its own.
    pub fn update_models_directory(&self, new_path: &str) -> Result<ModelsDirectoryInfo, GuiError> {
        set_models_dir(new_path, DirectoryCreationStrategy::AutoCreate).map_err(|e| match e {
            PathError::EnvFileError { .. } | PathError::NoDataDir => {
                GuiError::Internal(format!("Failed to save models dir: {e}"))
            }
            refused => GuiError::ValidationFailed(refused.to_string()),
        })?;
        self.get_models_directory_info()
    }

    /// Get current application settings.
    pub async fn get(&self) -> Result<AppSettings, GuiError> {
        let settings = self.deps.core.settings().get().await?;

        Ok(settings.into())
    }

    /// Update application settings with validation.
    pub async fn update(&self, request: UpdateSettingsRequest) -> Result<AppSettings, GuiError> {
        let update: SettingsUpdate = request.clone().into();

        let settings = self.deps.core.settings().update(update).await?;

        if let Some(Some(queue_size)) = request.max_download_queue_size {
            let _ = self.deps.downloads.set_max_queue_size(queue_size).await;
        }

        Ok(settings.into())
    }

    /// Get system memory information.
    ///
    /// Returns None if memory information is unavailable (probe failed, too small, etc.).
    pub fn get_system_memory(&self) -> Result<Option<SystemMemoryInfo>, GuiError> {
        Ok(system_memory(self.deps.system_probe.as_ref()))
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "settings_models_dir_tests.rs"]
mod models_dir_tests;
