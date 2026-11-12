//! Settings operations for GUI backend.

use std::sync::Arc;

use gglib_core::SettingsUpdate;
use gglib_core::domain::ModelsDirState;
use gglib_core::paths::{
    ModelsDirSource, PathError, can_write_folder, default_models_dir, resolve_models_dir,
    set_models_dir,
};
use gglib_core::ports::{DownloadManagerPort, SystemProbePort};
use gglib_core::services::AppCore;
use gglib_core::utils::system::SystemMemoryInfo;

use crate::error::GuiError;
use crate::types::{AppSettings, InstalledTemplates, ModelsDirectoryInfo, UpdateSettingsRequest};

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
/// The one description of the directory, for the settings page, the setup
/// status and `gglib config show` alike. The default is core's
/// own, and is blank where there is no home directory to put one under.
pub fn models_directory_info() -> Result<ModelsDirectoryInfo, PathError> {
    let resolution = resolve_models_dir(None)?;
    Ok(directory_info_at(
        &resolution.path,
        format_source(resolution.source),
        default_models_dir()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default(),
    ))
}

/// The description of the folder at `path`, chosen from `source`: whether
/// it is there, whether it can be written (made, for one that is not
/// there: `can_write_folder` writes a probe to it or to the folder it would
/// be made in), and so its state.
fn directory_info_at(
    path: &std::path::Path,
    source: &str,
    default_path: String,
) -> ModelsDirectoryInfo {
    let exists = path.exists();
    let writable = can_write_folder(path);
    ModelsDirectoryInfo {
        path: path.to_string_lossy().to_string(),
        source: source.to_owned(),
        default_path,
        exists,
        writable,
        state: ModelsDirState::of(exists, writable),
    }
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
    /// The operation `gglib config set models-dir` runs. What it stores is
    /// read by every later run, and by this one unless its environment names
    /// a directory of its own.
    pub fn update_models_directory(&self, new_path: &str) -> Result<ModelsDirectoryInfo, GuiError> {
        set_models_dir(new_path).map_err(|e| match e {
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

        // The running download manager holds the size it was built with. A
        // write that names the field, setting it or clearing it, hands the
        // manager the size now stored.
        if request.max_download_queue_size.is_some() {
            let size = settings.effective_max_download_queue_size();
            if let Err(e) = self.deps.downloads.set_max_queue_size(size).await {
                tracing::warn!("the download queue keeps its size, not the stored {size}: {e}");
            }
        }

        Ok(settings.into())
    }

    /// Add the starter profiles to the stored list, keeping any stored
    /// profile that has a template's name.
    ///
    /// What the settings page's button does, by the function `gglib config
    /// profile install-templates` runs. Only the command can put a template
    /// in a stored profile's place, with `--force`.
    pub async fn install_profile_templates(&self) -> Result<InstalledTemplates, GuiError> {
        let settings = self.deps.core.settings();
        let (settings, done) = settings.install_profile_templates(false).await?;

        Ok(InstalledTemplates {
            installed: done.installed,
            kept: done.kept,
            settings: settings.into(),
        })
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "settings_models_dir_tests.rs"]
mod models_dir_tests;

#[cfg(test)]
#[path = "settings_drawing_tests.rs"]
mod drawing_tests;
