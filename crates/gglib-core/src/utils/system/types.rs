//! System dependency and GPU detection types.

use serde::{Deserialize, Serialize};

/// Represents the status of a system dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyStatus {
    /// Dependency is installed and available.
    Present { version: String },
    /// Dependency is missing.
    Missing,
    /// Dependency is optional (not required for basic functionality).
    Optional,
}

/// Information about a system dependency.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Name of the dependency (e.g., "cargo", "node").
    pub name: String,
    /// Current status of the dependency.
    pub status: DependencyStatus,
    /// Description of what this dependency is used for.
    pub description: String,
    /// Whether this dependency is required or optional.
    pub required: bool,
    /// Installation instructions or hints.
    pub install_hint: Option<String>,
}

impl Dependency {
    /// Create a new required dependency.
    pub fn required(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DependencyStatus::Missing,
            description: description.into(),
            required: true,
            install_hint: None,
        }
    }

    /// Create a new optional dependency.
    pub fn optional(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DependencyStatus::Optional,
            description: description.into(),
            required: false,
            install_hint: None,
        }
    }

    /// Set installation hint.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.install_hint = Some(hint.into());
        self
    }

    /// Set the status of this dependency.
    #[must_use]
    pub fn with_status(mut self, status: DependencyStatus) -> Self {
        self.status = status;
        self
    }
}

/// GPU hardware detection result.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInfo {
    /// NVIDIA GPU hardware detected (via nvidia-smi, lspci, etc.).
    pub has_nvidia_gpu: bool,
    /// CUDA toolkit installed and available.
    pub cuda_version: Option<String>,
    /// On macOS (Metal always available).
    pub has_metal: bool,
    /// Vulkan runtime available (AMD, Intel, NVIDIA via Mesa/drivers).
    pub has_vulkan: bool,
    /// Vulkan development headers installed (`vulkan/vulkan.h`).
    pub vulkan_headers: bool,
    /// SPIR-V shader compiler (`glslc`) available.
    pub vulkan_glslc: bool,
    /// SPIR-V headers installed (`spirv/unified1/spirv.hpp`).
    ///
    /// Required by llama.cpp's `ggml-vulkan.cpp` at build time. Ships
    /// as a separate package on Linux (e.g. `spirv-headers`) and is
    /// bundled in the `LunarG` Vulkan SDK on Windows.
    pub vulkan_spirv_headers: bool,
}

/// System memory information for model fit calculations.
///
/// Also the one shape memory goes over HTTP in, from the settings route and
/// inside the setup status: camelCase, and no `gpuMemoryBytes` key at all
/// where the figure could not be read.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SystemMemoryInfo {
    /// Total system RAM in bytes.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub total_ram_bytes: u64,
    /// GPU memory in bytes: VRAM on a discrete card, or the addressable share
    /// of host RAM on a unified-memory device (Apple Silicon, or an integrated
    /// GPU). None if no GPU was detected or its memory could not be read.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_bytes: Option<u64>,
    /// Whether the GPU shares host memory — Apple Silicon, or an integrated
    /// GPU whose heaps are GTT rather than its own VRAM. Decides whether the
    /// figure above is labelled "VRAM" or "unified memory" to the user.
    pub is_unified_memory: bool,
    /// Whether the system has an NVIDIA GPU.
    pub has_nvidia_gpu: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names the frontend reads memory by, on every route that sends it.
    #[test]
    fn memory_goes_over_the_wire_in_camel_case() {
        let json = serde_json::to_value(SystemMemoryInfo {
            total_ram_bytes: 1024,
            gpu_memory_bytes: Some(512),
            is_unified_memory: true,
            has_nvidia_gpu: false,
        })
        .unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "totalRamBytes": 1024,
                "gpuMemoryBytes": 512,
                "isUnifiedMemory": true,
                "hasNvidiaGpu": false,
            })
        );
    }

    /// A GPU figure that could not be read has no key, not a `null`: the
    /// generated type says the key is optional, and a reader has one absent
    /// shape to handle.
    #[test]
    fn an_unread_gpu_figure_has_no_key() {
        let json = serde_json::to_value(SystemMemoryInfo {
            total_ram_bytes: 1024,
            gpu_memory_bytes: None,
            is_unified_memory: false,
            has_nvidia_gpu: false,
        })
        .unwrap();

        assert!(json.get("gpuMemoryBytes").is_none(), "{json}");
    }
}
