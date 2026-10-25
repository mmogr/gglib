#![doc = include_str!("README.md")]
#[allow(
    clippy::manual_let_else,
    clippy::unnecessary_wraps,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod cuda;
mod metal;
pub(crate) mod tools;
mod vulkan;

// Re-export submodule public API
#[cfg(target_os = "linux")]
pub(super) use cuda::select_cuda_compiler_for_build;
pub(super) use cuda::{get_cuda_path, validate_cuda_gcc_compatibility};
pub(super) use tools::get_num_cores;
pub use vulkan::{MissingPackage, VulkanStatus, vulkan_status};

use anyhow::{Result, anyhow};

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
            Self::Metal => "Metal",
            Self::Cuda => "CUDA",
            Self::Vulkan => "Vulkan",
            Self::Cpu => "CPU",
        }
    }

    /// Get the `CMake` flags for this acceleration type.
    pub fn cmake_flags(&self) -> Vec<&str> {
        match self {
            Self::Metal => vec!["-DGGML_METAL=ON"],
            Self::Cuda => vec!["-DGGML_CUDA=ON"],
            Self::Vulkan => vec!["-DGGML_VULKAN=ON"],
            Self::Cpu => vec![],
        }
    }
}

impl std::fmt::Display for Acceleration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Detect the optimal acceleration type for the current system.
///
/// Metal, CUDA, or Vulkan when one is **fully buildable**. For Vulkan, that
/// means the loader, headers, `glslc`, **and** SPIR-V headers are all
/// present (see [`VulkanStatus::ready_for_build`]). With no GPU at all, the
/// CPU: the build runs, slowly, and the caller says so.
///
/// When a Vulkan GPU is there but its build dependencies are incomplete
/// (e.g. the loader and a device present but SPIR-V headers missing), this
/// returns `Err` so the caller can surface install hints — we do not
/// silently degrade to CPU when the user has a usable GPU.
pub fn detect_optimal_acceleration() -> Result<Acceleration> {
    if cfg!(target_os = "macos") && metal::has_metal_support() {
        Ok(Acceleration::Metal)
    } else if cuda::has_cuda_toolkit() {
        Ok(Acceleration::Cuda)
    } else {
        without_metal_or_cuda(
            &vulkan_status(),
            crate::system::gpu::vulkan_device_present(),
        )
    }
}

/// The choice with neither Metal nor CUDA, from what `vulkan` found and
/// whether a GPU `device` is there: Vulkan when both its build is ready and
/// a device is there to run it, a refusal naming what is missing when only
/// the build is not ready, and the CPU otherwise. A Vulkan build with no
/// device would run on the CPU and never say so.
fn without_metal_or_cuda(vulkan: &VulkanStatus, device: bool) -> Result<Acceleration> {
    match (device && vulkan.has_loader, vulkan.ready_for_build()) {
        (true, true) => Ok(Acceleration::Vulkan),
        (true, false) => Err(vulkan_unbuildable(vulkan)),
        (false, _) => Ok(Acceleration::Cpu),
    }
}

/// Why a Vulkan GPU that is there is not built for: the packages its build
/// also needs, and the command that lists how to install them.
fn vulkan_unbuildable(vulkan: &VulkanStatus) -> anyhow::Error {
    let missing: Vec<&str> = vulkan.missing.iter().map(MissingPackage::label).collect();
    anyhow!(
        "A Vulkan GPU was detected, but a Vulkan build also needs: {}.\n\
         Run 'gglib config llama detect' to see how to install them.\n\
         gglib will not fall back to a CPU-only build while a GPU is there.",
        missing.join(", ")
    )
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

    fn ready() -> VulkanStatus {
        VulkanStatus {
            has_loader: true,
            has_headers: true,
            has_glslc: true,
            has_spirv_headers: true,
            missing: Vec::new(),
        }
    }

    /// Vulkan needs both its build and a device; with no device, every build
    /// package installed or not, the CPU.
    #[test]
    fn vulkan_is_chosen_only_where_a_device_runs_it() {
        assert_eq!(
            without_metal_or_cuda(&ready(), true).unwrap(),
            Acceleration::Vulkan
        );
        assert_eq!(
            without_metal_or_cuda(&ready(), false).unwrap(),
            Acceleration::Cpu
        );
        assert_eq!(
            without_metal_or_cuda(&VulkanStatus::absent(), true).unwrap(),
            Acceleration::Cpu
        );
        let missing = VulkanStatus {
            has_glslc: false,
            missing: vec![MissingPackage::Glslc],
            ..ready()
        };
        assert!(without_metal_or_cuda(&missing, true).is_err());
        assert_eq!(
            without_metal_or_cuda(&missing, false).unwrap(),
            Acceleration::Cpu
        );
    }

    /// A Vulkan GPU with part of its build missing is told each missing
    /// package and the command that says how to get it.
    #[test]
    fn a_vulkan_gpu_without_its_build_packages_is_told_what_is_missing() {
        let loader_only = VulkanStatus {
            has_loader: true,
            has_headers: true,
            has_glslc: false,
            has_spirv_headers: false,
            missing: vec![MissingPackage::Glslc, MissingPackage::SpirvHeaders],
        };
        assert_eq!(
            vulkan_unbuildable(&loader_only).to_string(),
            "A Vulkan GPU was detected, but a Vulkan build also needs: \
             SPIR-V shader compiler (glslc), SPIR-V headers (spirv-headers).\n\
             Run 'gglib config llama detect' to see how to install them.\n\
             gglib will not fall back to a CPU-only build while a GPU is there."
        );
    }

    #[test]
    #[allow(
        clippy::single_match_else,
        reason = "grandfathered at lint inheritance, #1157"
    )]
    fn test_detect_optimal_acceleration() {
        match detect_optimal_acceleration() {
            Ok(accel) => {
                // Any of them, the CPU included on a machine with no GPU.
                let _ = accel.display_name();
            }
            Err(_) => {
                // A Vulkan GPU whose build packages are missing.
            }
        }
    }
}
