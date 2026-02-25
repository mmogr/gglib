//! Setup wizard operations for GUI backend.
//!
//! Handles first-run system status checks, llama.cpp installation,
//! and Python fast-download helper provisioning.

use serde::Serialize;

use crate::deps::GuiDeps;
use crate::error::GuiError;

/// Combined setup status returned by the setup-status endpoint.
///
/// Provides everything the frontend wizard needs to render its initial state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    /// Whether the setup wizard has been completed previously.
    pub setup_completed: bool,
    /// Whether llama-server and llama-cli binaries are installed.
    pub llama_installed: bool,
    /// Whether pre-built binaries can be downloaded for this platform.
    pub llama_can_download: bool,
    /// Platform description for pre-built binaries (e.g., "macOS ARM64 (Metal)").
    pub llama_platform_description: Option<String>,
    /// GPU information.
    pub gpu_info: GpuInfoDto,
    /// Models directory information.
    pub models_directory: ModelsDirectoryDto,
    /// Whether Python 3 is available on the system.
    pub python_available: bool,
    /// Whether the fast download helper (hf_xet venv) is ready.
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
    pub cuda_version: Option<String>,
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
    pub is_apple_silicon: bool,
}

/// Setup operations handler.
pub struct SetupOps<'a> {
    deps: &'a GuiDeps,
}

impl<'a> SetupOps<'a> {
    pub fn new(deps: &'a GuiDeps) -> Self {
        Self { deps }
    }

    /// Get the full setup status for the wizard.
    pub async fn get_status(&self) -> Result<SetupStatus, GuiError> {
        // Check if setup was previously completed
        let settings = self
            .deps
            .settings()
            .get()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to get settings: {e}")))?;
        let setup_completed = settings.setup_completed.unwrap_or(false);

        // Check llama installation
        let llama_installed = gglib_runtime::llama::check_llama_installed();

        // Check prebuilt availability
        let (llama_can_download, llama_platform_description) = {
            use gglib_runtime::llama::{
                PrebuiltAvailability, check_prebuilt_availability,
            };
            match check_prebuilt_availability() {
                PrebuiltAvailability::Available { description, .. } => {
                    (true, Some(description))
                }
                PrebuiltAvailability::NotAvailable { .. } => (false, None),
            }
        };

        // GPU detection
        let gpu_info_raw = self.deps.system_probe.detect_gpu_info();
        let gpu_info = GpuInfoDto {
            has_metal: gpu_info_raw.has_metal,
            has_nvidia: gpu_info_raw.has_nvidia_gpu,
            cuda_version: gpu_info_raw.cuda_version,
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

        // Python / fast download helper
        let python_available = gglib_download::cli_exec::preflight_fast_helper()
            .await
            .is_ok();

        // Check if the venv python exists (fast check, no process spawn)
        let fast_download_ready = python_available && is_python_venv_ready();

        // System memory
        let mem_info = self.deps.system_probe.get_system_memory_info();
        let system_memory = if mem_info.total_ram_bytes > 256 * 1024 * 1024 {
            Some(SystemMemoryDto {
                total_ram_bytes: mem_info.total_ram_bytes,
                gpu_memory_bytes: mem_info.gpu_memory_bytes,
                is_apple_silicon: mem_info.is_apple_silicon,
            })
        } else {
            None
        };

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

    /// Install llama.cpp pre-built binaries with a progress callback.
    ///
    /// Returns an error if pre-built binaries are not available for this platform.
    pub async fn install_llama(
        &self,
        progress_callback: gglib_runtime::llama::LlamaProgressCallbackBoxed,
    ) -> Result<(), GuiError> {
        gglib_runtime::llama::download_prebuilt_binaries_with_boxed_callback(
            progress_callback,
        )
        .await
        .map_err(|e| GuiError::Internal(format!("Failed to install llama.cpp: {e}")))
    }

    /// Provision the Python fast-download helper environment.
    ///
    /// Creates a venv and installs huggingface_hub + hf_xet packages.
    /// Returns an error with details if Python is not available or setup fails.
    pub async fn setup_python_env(&self) -> Result<(), GuiError> {
        gglib_download::cli_exec::ensure_fast_helper_ready()
            .await
            .map_err(|e| GuiError::Internal(format!("Failed to setup Python environment: {e}")))
    }
}

/// Check if the Python fast-download venv is already provisioned.
///
/// Does a quick file-existence check for the venv Python binary
/// at the well-known path `data_root()/.conda/gglib-hf-xet/bin/python3`.
fn is_python_venv_ready() -> bool {
    let Ok(root) = gglib_core::paths::data_root() else {
        return false;
    };
    let venv_dir = root.join(".conda").join("gglib-hf-xet");
    if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe").exists()
    } else {
        venv_dir.join("bin").join("python3").exists()
    }
}
