//! System information DTOs.

use gglib_core::utils::system::SystemMemoryInfo;
use gglib_runtime::llama::{MissingPackage, VulkanStatus};
use serde::{Deserialize, Serialize};

/// System memory information DTO for HTTP API.
///
/// This DTO ensures stable JSON field names (camelCase) for frontend consumption.
/// Uses the memory field most useful for model fit calculations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemMemoryInfoDto {
    /// Total system RAM in bytes.
    pub total_ram_bytes: u64,
    /// GPU memory in bytes (VRAM for discrete GPUs, or unified memory portion for Apple Silicon).
    /// None if no GPU detected or memory couldn't be determined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_bytes: Option<u64>,
    /// Whether the system has Apple Silicon with unified memory.
    pub is_apple_silicon: bool,
    /// Whether the system has an NVIDIA GPU.
    pub has_nvidia_gpu: bool,
}

impl From<SystemMemoryInfo> for SystemMemoryInfoDto {
    fn from(info: SystemMemoryInfo) -> Self {
        Self {
            total_ram_bytes: info.total_ram_bytes,
            gpu_memory_bytes: info.gpu_memory_bytes,
            is_apple_silicon: info.is_apple_silicon,
            has_nvidia_gpu: info.has_nvidia_gpu,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dto_from_core_type() {
        let core_info = SystemMemoryInfo {
            total_ram_bytes: 16 * 1024 * 1024 * 1024,       // 16 GB
            gpu_memory_bytes: Some(8 * 1024 * 1024 * 1024), // 8 GB VRAM
            is_apple_silicon: false,
            has_nvidia_gpu: true,
        };

        let dto: SystemMemoryInfoDto = core_info.into();

        assert_eq!(dto.total_ram_bytes, 16 * 1024 * 1024 * 1024);
        assert_eq!(dto.gpu_memory_bytes, Some(8 * 1024 * 1024 * 1024));
        assert!(!dto.is_apple_silicon);
        assert!(dto.has_nvidia_gpu);
    }

    #[test]
    fn test_dto_serialization_camel_case() {
        let dto = SystemMemoryInfoDto {
            total_ram_bytes: 1024,
            gpu_memory_bytes: Some(512),
            is_apple_silicon: true,
            has_nvidia_gpu: false,
        };

        let json = serde_json::to_value(&dto).unwrap();

        assert!(json.get("totalRamBytes").is_some());
        assert!(json.get("gpuMemoryBytes").is_some());
        assert!(json.get("isAppleSilicon").is_some());
        assert!(json.get("hasNvidiaGpu").is_some());

        // Ensure snake_case fields don't exist
        assert!(json.get("total_ram_bytes").is_none());
        assert!(json.get("gpu_memory_bytes").is_none());
    }

    #[test]
    fn test_dto_none_gpu_omitted() {
        let dto = SystemMemoryInfoDto {
            total_ram_bytes: 1024,
            gpu_memory_bytes: None,
            is_apple_silicon: false,
            has_nvidia_gpu: false,
        };

        let json = serde_json::to_string(&dto).unwrap();

        // GPU memory should be omitted when None
        assert!(!json.contains("gpuMemoryBytes"));
    }
}

/// Vulkan build-readiness status DTO for HTTP API.
///
/// Mirrors [`VulkanStatus`] from `gglib-runtime` with stable camelCase
/// JSON field names for frontend consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VulkanStatusDto {
    /// Vulkan runtime loader is present and working.
    pub has_loader: bool,
    /// Vulkan development headers are installed.
    pub has_headers: bool,
    /// The `glslc` SPIR-V shader compiler is available.
    pub has_glslc: bool,
    /// SPIR-V headers (`spirv/unified1/spirv.hpp`) are installed.
    pub has_spirv_headers: bool,
    /// Whether all components needed for a Vulkan build are present.
    pub ready_for_build: bool,
    /// Components that are missing (empty if fully ready).
    pub missing: Vec<MissingPackageDto>,
}

impl From<VulkanStatus> for VulkanStatusDto {
    fn from(status: VulkanStatus) -> Self {
        let ready_for_build = status.ready_for_build();
        Self {
            has_loader: status.has_loader,
            has_headers: status.has_headers,
            has_glslc: status.has_glslc,
            has_spirv_headers: status.has_spirv_headers,
            ready_for_build,
            missing: status.missing.into_iter().map(Into::into).collect(),
        }
    }
}

/// A missing Vulkan build component with install hints.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingPackageDto {
    /// Machine-readable identifier for the missing component.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// Distro-specific install commands.
    pub install_hints: Vec<InstallHintDto>,
}

impl From<MissingPackage> for MissingPackageDto {
    fn from(pkg: MissingPackage) -> Self {
        let id = match &pkg {
            MissingPackage::VulkanLoader => "vulkanLoader",
            MissingPackage::VulkanHeaders => "vulkanHeaders",
            MissingPackage::Glslc => "glslc",
            MissingPackage::SpirvHeaders => "spirvHeaders",
        };
        Self {
            id: id.to_string(),
            label: pkg.label().to_string(),
            install_hints: pkg
                .install_hints()
                .into_iter()
                .map(|(distro, cmd)| InstallHintDto {
                    distro: distro.to_string(),
                    command: cmd.to_string(),
                })
                .collect(),
        }
    }
}

/// Distro-specific install command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallHintDto {
    /// Distribution label (e.g. "Ubuntu/Debian", "Arch").
    pub distro: String,
    /// Shell command to install the missing component.
    pub command: String,
}

#[cfg(test)]
mod vulkan_dto_tests {
    use super::*;

    #[test]
    fn test_vulkan_status_dto_from_ready() {
        let status = VulkanStatus {
            has_loader: true,
            has_headers: true,
            has_glslc: true,
            has_spirv_headers: true,
            missing: vec![],
        };
        let dto: VulkanStatusDto = status.into();
        assert!(dto.ready_for_build);
        assert!(dto.missing.is_empty());
    }

    #[test]
    fn test_vulkan_status_dto_from_missing_headers() {
        let status = VulkanStatus {
            has_loader: true,
            has_headers: false,
            has_glslc: true,
            has_spirv_headers: true,
            missing: vec![MissingPackage::VulkanHeaders],
        };
        let dto: VulkanStatusDto = status.into();
        assert!(!dto.ready_for_build);
        assert_eq!(dto.missing.len(), 1);
        assert_eq!(dto.missing[0].id, "vulkanHeaders");
        assert!(!dto.missing[0].install_hints.is_empty());
    }

    #[test]
    fn test_vulkan_status_dto_from_missing_spirv_headers() {
        let status = VulkanStatus {
            has_loader: true,
            has_headers: true,
            has_glslc: true,
            has_spirv_headers: false,
            missing: vec![MissingPackage::SpirvHeaders],
        };
        let dto: VulkanStatusDto = status.into();
        assert!(!dto.ready_for_build);
        assert!(!dto.has_spirv_headers);
        assert_eq!(dto.missing.len(), 1);
        assert_eq!(dto.missing[0].id, "spirvHeaders");
        assert!(!dto.missing[0].install_hints.is_empty());
    }

    #[test]
    fn test_vulkan_status_dto_camel_case() {
        let status = VulkanStatus {
            has_loader: true,
            has_headers: false,
            has_glslc: false,
            has_spirv_headers: false,
            missing: vec![
                MissingPackage::VulkanHeaders,
                MissingPackage::Glslc,
                MissingPackage::SpirvHeaders,
            ],
        };
        let dto: VulkanStatusDto = status.into();
        let json = serde_json::to_value(&dto).unwrap();
        assert!(json.get("hasLoader").is_some());
        assert!(json.get("hasHeaders").is_some());
        assert!(json.get("hasGlslc").is_some());
        assert!(json.get("hasSpirvHeaders").is_some());
        assert!(json.get("readyForBuild").is_some());
        assert!(json.get("missing").is_some());
    }
}
