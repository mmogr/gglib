//! Hardware acceleration detection for llama.cpp builds.
//!
//! This module detects which GPU acceleration backend (Metal, CUDA, or
//! Vulkan) is available on the current system and selects the optimal
//! one for building llama.cpp.
//!
//! # Submodules
//!
//! | Module | Responsibility |
//! |--------|---------------|
//! | [`tools`] | Shared command-execution and version-parsing utilities |
//! | [`metal`] | Apple Metal detection (macOS only) |
//! | [`cuda`] | NVIDIA CUDA toolkit detection and GCC compatibility |
//! | [`vulkan`] | Vulkan loader, header, and `glslc` detection |
//!
//! # Priority order
//!
//! [`detect_optimal_acceleration`] selects backends in this priority:
//!
//! 1. **Metal** — macOS with Apple Silicon or Intel Mac ≥10.13
//! 2. **CUDA** — NVIDIA GPU with `nvcc` in `PATH`
//! 3. **Vulkan** — AMD/Intel/NVIDIA via portable GPU API (runtime only)
//!
//! CPU-only inference is not supported.

mod cuda;
mod metal;
pub(crate) mod tools;
mod vulkan;

// Re-export submodule public API
#[cfg(target_os = "linux")]
pub use cuda::select_cuda_compiler_for_build;
#[cfg(feature = "cli")]
pub use cuda::{get_cuda_path, validate_cuda_gcc_compatibility};
#[cfg(feature = "cli")]
pub use tools::{get_num_cores, has_cmake, has_cpp_compiler, has_git};
pub use vulkan::{MissingPackage, VulkanStatus, vulkan_status};

use anyhow::{Result, bail};

/// Acceleration type for llama.cpp build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceleration {
    /// Metal acceleration (Apple Silicon).
    Metal,
    /// CUDA acceleration (NVIDIA).
    Cuda,
    /// Vulkan acceleration (AMD, Intel, NVIDIA via portable GPU API).
    Vulkan,
    /// CPU only (no acceleration).
    Cpu,
}

impl Acceleration {
    /// Get the display name for this acceleration type.
    pub fn display_name(&self) -> &str {
        match self {
            Acceleration::Metal => "Metal",
            Acceleration::Cuda => "CUDA",
            Acceleration::Vulkan => "Vulkan",
            Acceleration::Cpu => "CPU",
        }
    }

    /// Get the CMake flags for this acceleration type.
    pub fn cmake_flags(&self) -> Vec<&str> {
        match self {
            Acceleration::Metal => vec!["-DGGML_METAL=ON"],
            Acceleration::Cuda => vec!["-DGGML_CUDA=ON"],
            Acceleration::Vulkan => vec!["-DGGML_VULKAN=ON"],
            Acceleration::Cpu => vec![],
        }
    }
}

impl std::fmt::Display for Acceleration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Detect the optimal acceleration type for the current system, strictly.
///
/// Returns an error if no supported GPU acceleration (Metal, CUDA, or
/// Vulkan) is **fully buildable**. For Vulkan, that means the loader,
/// headers, `glslc`, **and** SPIR-V headers are all present (see
/// [`VulkanStatus::ready_for_build`]).
///
/// When a GPU runtime is detected but the build dependencies are
/// incomplete (e.g. Vulkan loader present but SPIR-V headers missing),
/// this returns `Err` so the caller can surface install hints — we do
/// not silently degrade to CPU when the user has a usable GPU.
pub fn detect_optimal_acceleration() -> Result<Acceleration> {
    if cfg!(target_os = "macos") && metal::has_metal_support() {
        Ok(Acceleration::Metal)
    } else if cuda::has_cuda_toolkit() {
        Ok(Acceleration::Cuda)
    } else if vulkan_status().ready_for_build() {
        Ok(Acceleration::Vulkan)
    } else {
        bail!(
            "No supported GPU acceleration found.\n\
             gglib requires Metal (macOS), CUDA (NVIDIA), or Vulkan (AMD/Intel) for inference.\n\
             CPU-only inference is not supported."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acceleration_display() {
        assert_eq!(Acceleration::Metal.display_name(), "Metal");
        assert_eq!(Acceleration::Cuda.display_name(), "CUDA");
        assert_eq!(Acceleration::Vulkan.display_name(), "Vulkan");
        assert_eq!(Acceleration::Cpu.display_name(), "CPU");
    }

    #[test]
    fn test_acceleration_cmake_flags() {
        assert_eq!(Acceleration::Metal.cmake_flags(), vec!["-DGGML_METAL=ON"]);
        assert_eq!(Acceleration::Cuda.cmake_flags(), vec!["-DGGML_CUDA=ON"]);
        assert_eq!(Acceleration::Vulkan.cmake_flags(), vec!["-DGGML_VULKAN=ON"]);
        assert!(Acceleration::Cpu.cmake_flags().is_empty());
    }

    #[test]
    fn test_detect_optimal_acceleration() {
        match detect_optimal_acceleration() {
            Ok(accel) => {
                assert!(matches!(
                    accel,
                    Acceleration::Metal | Acceleration::Cuda | Acceleration::Vulkan
                ));
            }
            Err(_) => {
                // No supported GPU on this machine — correct behavior
            }
        }
    }
}
