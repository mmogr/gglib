//! System information DTOs.

use gglib_core::utils::system::SystemMemoryInfo;
use serde::{Deserialize, Serialize};

/// System memory information DTO for HTTP API.
///
/// This DTO ensures stable JSON field names (camelCase) for frontend consumption.
/// Uses the memory field most useful for model fit calculations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub(crate) struct SystemMemoryInfoDto {
    /// Total system RAM in bytes.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub total_ram_bytes: u64,
    /// GPU memory in bytes: VRAM on a discrete card, or the addressable share
    /// of host RAM on a unified-memory device. None if it could not be read.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_bytes: Option<u64>,
    /// Whether the GPU shares host memory — Apple Silicon or an integrated GPU.
    pub is_unified_memory: bool,
    /// Whether the system has an NVIDIA GPU.
    pub has_nvidia_gpu: bool,
}

impl From<SystemMemoryInfo> for SystemMemoryInfoDto {
    fn from(info: SystemMemoryInfo) -> Self {
        Self {
            total_ram_bytes: info.total_ram_bytes,
            gpu_memory_bytes: info.gpu_memory_bytes,
            is_unified_memory: info.is_unified_memory,
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
            is_unified_memory: false,
            has_nvidia_gpu: true,
        };

        let dto: SystemMemoryInfoDto = core_info.into();

        assert_eq!(dto.total_ram_bytes, 16 * 1024 * 1024 * 1024);
        assert_eq!(dto.gpu_memory_bytes, Some(8 * 1024 * 1024 * 1024));
        assert!(!dto.is_unified_memory);
        assert!(dto.has_nvidia_gpu);
    }

    #[test]
    fn test_dto_serialization_camel_case() {
        let dto = SystemMemoryInfoDto {
            total_ram_bytes: 1024,
            gpu_memory_bytes: Some(512),
            is_unified_memory: true,
            has_nvidia_gpu: false,
        };

        let json = serde_json::to_value(&dto).unwrap();

        assert!(json.get("totalRamBytes").is_some());
        assert!(json.get("gpuMemoryBytes").is_some());
        assert!(json.get("isUnifiedMemory").is_some());
        assert!(json.get("hasNvidiaGpu").is_some());

        // Ensure snake_case fields don't exist
        assert!(json.get("total_ram_bytes").is_none());
        assert!(json.get("gpu_memory_bytes").is_none());

        // The field was `isAppleSilicon` until integrated GPUs started
        // reporting a budget too. It answers "does the GPU share host memory",
        // which is what every consumer was already asking it.
        assert!(json.get("isAppleSilicon").is_none());
    }

    #[test]
    fn test_dto_none_gpu_omitted() {
        let dto = SystemMemoryInfoDto {
            total_ram_bytes: 1024,
            gpu_memory_bytes: None,
            is_unified_memory: false,
            has_nvidia_gpu: false,
        };

        let json = serde_json::to_string(&dto).unwrap();

        // GPU memory should be omitted when None
        assert!(!json.contains("gpuMemoryBytes"));
    }
}
