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
    } else {
        let vulkan = vulkan_status();
        if vulkan.ready_for_build() {
            Ok(Acceleration::Vulkan)
        } else {
            Err(no_acceleration(&vulkan))
        }
    }
}

/// Why no acceleration was chosen.
///
/// A Vulkan GPU that is there, with part of what a Vulkan build needs
/// missing, is the one refusal with a remedy to name: the packages, and the
/// command that lists how to install them.
fn no_acceleration(vulkan: &VulkanStatus) -> anyhow::Error {
    if vulkan.has_loader {
        let missing: Vec<&str> = vulkan.missing.iter().map(MissingPackage::label).collect();
        return anyhow!(
            "A Vulkan GPU was detected, but a Vulkan build also needs: {}.\n\
             Run 'gglib config llama detect' to see how to install them.\n\
             gglib will not fall back to a CPU-only build.",
            missing.join(", ")
        );
    }
    anyhow!(
        "No supported GPU acceleration found.\n\
         gglib requires Metal (macOS), CUDA (NVIDIA), or Vulkan (AMD/Intel) for inference.\n\
         CPU-only inference is not supported."
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

    /// With no GPU there is nothing to install, and with a Vulkan one there
    /// is: the refusal names each missing package and the command that says
    /// how to get it.
    #[test]
    fn a_vulkan_gpu_without_its_build_packages_is_told_what_is_missing() {
        let none = no_acceleration(&VulkanStatus::absent()).to_string();
        assert!(
            none.starts_with("No supported GPU acceleration found."),
            "{none}"
        );
        assert!(!none.contains("llama detect"), "{none}");

        let loader_only = VulkanStatus {
            has_loader: true,
            has_headers: true,
            has_glslc: false,
            has_spirv_headers: false,
            missing: vec![MissingPackage::Glslc, MissingPackage::SpirvHeaders],
        };
        assert_eq!(
            no_acceleration(&loader_only).to_string(),
            "A Vulkan GPU was detected, but a Vulkan build also needs: \
             SPIR-V shader compiler (glslc), SPIR-V headers (spirv-headers).\n\
             Run 'gglib config llama detect' to see how to install them.\n\
             gglib will not fall back to a CPU-only build."
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
