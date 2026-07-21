//! Settings operations for GUI backend.

use std::sync::Arc;

use gglib_core::SettingsUpdate;
use gglib_core::paths::{ModelsDirSource, resolve_models_dir};
use gglib_core::ports::{DownloadManagerPort, SystemProbePort};
use gglib_core::services::AppCore;
use gglib_core::utils::system::SystemMemoryInfo;

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
            .core
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
            default_model_id: settings.default_model_id,
            inference_defaults: settings.inference_defaults,
            setup_completed: settings.setup_completed,
            title_generation_prompt: settings.title_generation_prompt,
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
            default_model_id: request.default_model_id,
            inference_defaults: request.inference_defaults,
            // Not yet carried by the HTTP API — wired up when
            // `UpdateSettingsRequest` gains the field. `None` means "leave
            // whatever is stored alone", so profiles set by other surfaces
            // survive a settings update from this one.
            inference_profiles: None,
            setup_completed: request.setup_completed,
            title_generation_prompt: request.title_generation_prompt,
        };

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

        Ok(AppSettings {
            default_download_path: settings.default_download_path,
            default_context_size: settings.default_context_size,
            proxy_port: settings.proxy_port,
            llama_base_port: settings.llama_base_port,
            max_download_queue_size: settings.max_download_queue_size,
            show_memory_fit_indicators: settings.show_memory_fit_indicators,
            max_tool_iterations: settings.max_tool_iterations,
            max_stagnation_steps: settings.max_stagnation_steps,
            default_model_id: settings.default_model_id,
            inference_defaults: settings.inference_defaults,
            setup_completed: settings.setup_completed,
            title_generation_prompt: settings.title_generation_prompt,
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::test_support::{MockDownloadManager, MockSystemProbePort, test_core};

    fn make_ops(core: Arc<AppCore>, probe: MockSystemProbePort) -> SettingsOps {
        SettingsOps::new(SettingsDeps {
            core,
            system_probe: Arc::new(probe),
            downloads: Arc::new(MockDownloadManager::new()),
        })
    }

    #[tokio::test]
    async fn get_returns_default_settings() {
        let core = test_core().await;
        let ops = make_ops(core, MockSystemProbePort::default());
        let settings = ops.get().await.expect("get should succeed");
        // Fresh DB: no custom settings – all optional fields are None
        assert!(settings.default_download_path.is_none());
    }

    #[tokio::test]
    async fn get_models_directory_info_returns_valid_info() {
        let core = test_core().await;
        let ops = make_ops(core, MockSystemProbePort::default());
        // This calls resolve_models_dir which is pure – should not panic
        let result = ops.get_models_directory_info();
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[tokio::test]
    async fn get_system_memory_returns_some_when_probe_reports_enough_ram() {
        let core = test_core().await;
        let probe = MockSystemProbePort {
            total_ram_bytes: 8 * 1024 * 1024 * 1024, // 8 GiB
        };
        let ops = make_ops(core, probe);
        let result = ops.get_system_memory().expect("should not error");
        assert!(result.is_some(), "expected Some for 8 GiB");
    }

    #[tokio::test]
    async fn get_system_memory_returns_none_when_probe_reports_tiny_ram() {
        let core = test_core().await;
        let probe = MockSystemProbePort {
            total_ram_bytes: 1024, // 1 KiB – suspiciously small
        };
        let ops = make_ops(core, probe);
        let result = ops.get_system_memory().expect("should not error");
        assert!(result.is_none(), "expected None for suspiciously small RAM");
    }

    /// JSON-boundary tests for `UpdateSettingsRequest`'s double-`Option`
    /// fields, mirroring the coverage added for
    /// `UpdateModelRequest.server_defaults`. Deserializes raw JSON (rather
    /// than constructing the struct in Rust) to prove
    /// `serde_with::rust::double_option` distinguishes an omitted key from
    /// an explicit `null` at the layer that actually matters.
    #[test]
    fn update_settings_request_omitted_field_is_none() {
        let req: UpdateSettingsRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(req.default_context_size, None, "omitted key must be None");
        assert_eq!(req.default_download_path, None, "omitted key must be None");
    }

    #[test]
    fn update_settings_request_explicit_null_is_some_none() {
        let req: UpdateSettingsRequest =
            serde_json::from_str(r#"{"defaultContextSize": null}"#).unwrap();
        assert_eq!(
            req.default_context_size,
            Some(None),
            "explicit null must clear the setting (Some(None))"
        );
    }

    #[test]
    fn update_settings_request_populated_value_is_some_some() {
        let req: UpdateSettingsRequest =
            serde_json::from_str(r#"{"defaultContextSize": 8192}"#).unwrap();
        assert_eq!(req.default_context_size, Some(Some(8192)));
    }

    /// End-to-end: drive `SettingsOps::update` with a real
    /// `serde_json::from_str` payload proving an explicit JSON `null`
    /// actually clears the setting through the full service+DB round trip,
    /// not just at deserialization.
    #[tokio::test]
    async fn update_settings_json_null_clears_default_download_path() {
        let core = test_core().await;
        let ops = make_ops(core, MockSystemProbePort::default());

        let set_req: UpdateSettingsRequest =
            serde_json::from_str(r#"{"defaultDownloadPath": "/custom/path"}"#).unwrap();
        let updated = ops.update(set_req).await.expect("update should succeed");
        assert_eq!(
            updated.default_download_path.as_deref(),
            Some("/custom/path")
        );

        let clear_req: UpdateSettingsRequest =
            serde_json::from_str(r#"{"defaultDownloadPath": null}"#).unwrap();
        let cleared = ops.update(clear_req).await.expect("update should succeed");
        assert!(
            cleared.default_download_path.is_none(),
            "explicit JSON null must clear default_download_path"
        );
    }
}
