//! Setup wizard operations for GUI backend.
//!
//! Handles first-run system status checks, llama.cpp installation,
//! and provisioning of the optional `hf_xet` download accelerator.

use std::sync::Arc;

use serde::Serialize;

use gglib_core::ports::SystemProbePort;
use gglib_core::services::AppCore;

use crate::error::GuiError;

/// Combined setup status returned by the setup-status endpoint.
///
/// Provides everything the frontend wizard needs to render its initial state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    /// Whether the setup wizard has been completed previously.
    pub setup_completed: bool,
    /// Whether llama-server binary is installed.
    pub llama_installed: bool,
    /// Whether pre-built binaries can be downloaded for this platform.
    pub llama_can_download: bool,
    /// Platform description for pre-built binaries (e.g., "macOS ARM64 (Metal)").
    pub llama_platform_description: Option<String>,
    /// GPU information.
    pub gpu_info: GpuInfoDto,
    /// Models directory information.
    pub models_directory: ModelsDirectoryDto,
    /// Whether Python 3 is available, i.e. whether the optional `hf_xet`
    /// accelerator *could* be provisioned. Downloading does not depend on it.
    pub python_available: bool,
    /// Whether the `hf_xet` accelerator is already provisioned.
    pub fast_download_ready: bool,
    /// System memory information.
    pub system_memory: Option<SystemMemoryDto>,
}

/// GPU detection results.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfoDto {
    pub has_metal: bool,
    pub has_nvidia: bool,
    pub has_vulkan: bool,
    pub cuda_version: Option<String>,
    pub vulkan_headers_installed: bool,
    pub vulkan_glslc_installed: bool,
    pub vulkan_spirv_headers_installed: bool,
}

/// Models directory status.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsDirectoryDto {
    pub path: String,
    pub exists: bool,
    pub writable: bool,
}

/// System memory summary.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemMemoryDto {
    pub total_ram_bytes: u64,
    pub gpu_memory_bytes: Option<u64>,
    pub is_unified_memory: bool,
}

/// Dependencies for setup operations.
pub struct SetupDeps {
    pub core: Arc<AppCore>,
    pub system_probe: Arc<dyn SystemProbePort>,
}

/// Setup operations handler.
pub struct SetupOps {
    deps: SetupDeps,
}

impl SetupOps {
    pub fn new(deps: SetupDeps) -> Self {
        Self { deps }
    }

    /// Get the full setup status for the wizard.
    pub async fn get_status(&self) -> Result<SetupStatus, GuiError> {
        // Check if setup was previously completed
        let settings = self
            .deps
            .core
            .settings()
            .get()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to get settings: {e}")))?;
        let mut setup_completed = settings.setup_completed.unwrap_or(false);

        // Check llama installation
        let llama_installed = gglib_runtime::llama::check_llama_installed();

        // Check prebuilt availability
        let (llama_can_download, llama_platform_description) = {
            use gglib_runtime::llama::{PrebuiltAvailability, check_prebuilt_availability};
            match check_prebuilt_availability() {
                PrebuiltAvailability::Available { description, .. } => (true, Some(description)),
                PrebuiltAvailability::NotAvailable { .. } => (false, None),
            }
        };

        // GPU detection
        let gpu_info_raw = self.deps.system_probe.detect_gpu_info();
        let gpu_info = GpuInfoDto {
            has_metal: gpu_info_raw.has_metal,
            has_nvidia: gpu_info_raw.has_nvidia_gpu,
            has_vulkan: gpu_info_raw.has_vulkan,
            cuda_version: gpu_info_raw.cuda_version,
            vulkan_headers_installed: gpu_info_raw.vulkan_headers,
            vulkan_glslc_installed: gpu_info_raw.vulkan_glslc,
            vulkan_spirv_headers_installed: gpu_info_raw.vulkan_spirv_headers,
        };

        // Models directory
        let models_directory = gglib_core::paths::resolve_models_dir(None)
            .map(|r| {
                let exists = r.path.exists();
                let writable = exists
                    && std::fs::metadata(&r.path)
                        .map(|m| !m.permissions().readonly())
                        .unwrap_or(false);
                ModelsDirectoryDto {
                    path: r.path.to_string_lossy().to_string(),
                    exists,
                    writable,
                }
            })
            .unwrap_or(ModelsDirectoryDto {
                path: String::new(),
                exists: false,
                writable: false,
            });

        // Optional hf_xet accelerator. Neither of these gates downloading —
        // that runs natively over HTTP — they only tell the wizard whether the
        // accelerator can be offered and whether it is already set up.
        let python_available = gglib_download::cli_exec::preflight_fast_helper()
            .await
            .is_ok();

        let fast_download_ready = gglib_download::cli_exec::fast_helper_provisioned();

        // System memory
        let mem_info = self.deps.system_probe.get_system_memory_info();
        let system_memory = if mem_info.total_ram_bytes > 256 * 1024 * 1024 {
            Some(SystemMemoryDto {
                total_ram_bytes: mem_info.total_ram_bytes,
                gpu_memory_bytes: mem_info.gpu_memory_bytes,
                is_unified_memory: mem_info.is_unified_memory,
            })
        } else {
            None
        };

        // Auto-complete setup if the system is already functional.
        // This avoids forcing users who build from source (or install via
        // cargo/package manager) through a redundant wizard — llama.cpp and
        // the models directory are already configured by the build/install
        // process, so there is nothing for the wizard to do.
        if !setup_completed
            && llama_installed
            && models_directory.exists
            && models_directory.writable
        {
            setup_completed = true;
            let update = gglib_core::settings::SettingsUpdate {
                setup_completed: Some(Some(true)),
                ..Default::default()
            };
            // Best-effort persist — a failure here must not block the user
            // from reaching the app.
            let _ = self.deps.core.settings().update(update).await;
        }

        Ok(SetupStatus {
            setup_completed,
            llama_installed,
            llama_can_download,
            llama_platform_description,
            gpu_info,
            models_directory,
            python_available,
            fast_download_ready,
            system_memory,
        })
    }

    /// Install llama.cpp pre-built binaries, streaming progress on `tx`.
    ///
    /// Returns an error if pre-built binaries are not available for this platform.
    pub async fn install_llama(
        &self,
        tx: tokio::sync::mpsc::Sender<gglib_runtime::llama::LlamaProgressEvent>,
    ) -> Result<(), GuiError> {
        gglib_runtime::llama::download_prebuilt_binaries(tx)
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to install llama.cpp: {e}")))
    }

    /// Provision the optional `hf_xet` download accelerator.
    ///
    /// Creates a venv and installs `huggingface_hub` + `hf_xet`. This is the
    /// only path that builds that environment — nothing provisions it
    /// implicitly, and downloads work without it.
    ///
    /// Returns an error with details if Python is not available or setup fails.
    pub async fn setup_python_env(&self) -> Result<(), GuiError> {
        gglib_download::cli_exec::ensure_fast_helper_ready()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to setup Python environment: {e}")))
    }

    /// Remove the accelerator's environment; downloads revert to native HTTP.
    ///
    /// The mirror of [`Self::setup_python_env`] — `gglib config fast-downloads
    /// disable`. Downloads keep working either way; this only removes the
    /// faster path.
    ///
    /// Returns whether anything was there to remove, so a caller can tell
    /// "disabled it" from "already off" rather than guessing.
    pub fn remove_python_env(&self) -> Result<bool, GuiError> {
        gglib_download::cli_exec::remove_fast_helper()
            .map_err(|e| GuiError::Internal(format!("Failed to remove Python environment: {e}")))
    }

    /// A model sized to this machine, from the same shortlist `gglib up` uses
    /// on a first run.
    ///
    /// `None` when nothing fits, which is a real answer rather than an error:
    /// a machine too small for the smallest candidate has to be told so, not
    /// handed a recommendation it cannot run.
    pub fn recommend_model(&self) -> Option<gglib_core::domain::recommendation::Recommendation> {
        let memory = self.deps.system_probe.get_system_memory_info();
        gglib_core::domain::recommendation::recommend(&memory)
    }

    /// Everything the diagnostics panel shows: dependency matrix, resolved
    /// paths, detected acceleration, accelerator state.
    ///
    /// Assembled in one call because they are read together — this is the
    /// "why isn't this working" surface, and someone comparing a missing
    /// dependency against a resolved path should not be watching four
    /// spinners. Individually cheap: no network, and no subprocess beyond the
    /// version probes `check_all_dependencies` already runs.
    pub fn diagnostics(&self) -> Result<Diagnostics, GuiError> {
        let dependencies = self.deps.system_probe.check_all_dependencies();

        let paths = gglib_core::paths::ResolvedPaths::resolve()
            .map_err(|e| GuiError::Internal(format!("Failed to resolve paths: {e}")))?;

        // Detection refuses to fall back to CPU so callers can surface install
        // hints. That refusal is an answer here, not a failed request.
        let acceleration = match gglib_runtime::llama::detect_optimal_acceleration() {
            Ok(accel) => AccelerationInfo {
                detected: Some(accel.display_name().to_string()),
                detection_error: None,
            },
            Err(e) => AccelerationInfo {
                detected: None,
                detection_error: Some(e.to_string()),
            },
        };

        let fast_downloads = match gglib_download::cli_exec::fast_helper_status() {
            Ok(status) => FastDownloadsInfo {
                provisioned: status.provisioned,
                env_dir: status.env_dir.display().to_string(),
                legacy_path: status.legacy_path,
                builder: status.builder,
                available_builder: status.available_builder.to_string(),
                error: None,
            },
            Err(e) => FastDownloadsInfo {
                provisioned: false,
                env_dir: String::new(),
                legacy_path: false,
                builder: None,
                available_builder: String::new(),
                error: Some(e.to_string()),
            },
        };

        Ok(Diagnostics {
            dependencies,
            paths,
            acceleration,
            fast_downloads,
        })
    }
}

/// What acceleration a build would use, with detection failure carried as data.
#[derive(Debug, Clone)]
pub struct AccelerationInfo {
    pub detected: Option<String>,
    pub detection_error: Option<String>,
}

/// The optional `hf_xet` download accelerator's state.
#[derive(Debug, Clone)]
pub struct FastDownloadsInfo {
    pub provisioned: bool,
    pub env_dir: String,
    pub legacy_path: bool,
    pub builder: Option<String>,
    pub available_builder: String,
    /// Why the status could not be read, when it could not.
    pub error: Option<String>,
}

/// The system diagnostics bundle — `gglib config check-deps`, `paths` and
/// `fast-downloads status` as one value.
#[derive(Debug, Clone)]
pub struct Diagnostics {
    pub dependencies: Vec<gglib_core::utils::system::Dependency>,
    pub paths: gglib_core::paths::ResolvedPaths,
    pub acceleration: AccelerationInfo,
    pub fast_downloads: FastDownloadsInfo,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::test_support::{MockSystemProbePort, test_core};

    #[tokio::test]
    async fn get_status_returns_ok_without_panicking() {
        let core = test_core().await;
        let ops = SetupOps::new(SetupDeps {
            core,
            system_probe: Arc::new(MockSystemProbePort::default()),
        });
        // get_status calls gglib_runtime directly; we only verify it returns Ok
        // (no panic, no internal unwrap) in a test environment.
        let result = ops.get_status().await;
        assert!(result.is_ok(), "get_status should not fail, got {result:?}");
    }
}
