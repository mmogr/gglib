//! System information DTOs.

use gglib_core::utils::system::SystemMemoryInfo;
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
