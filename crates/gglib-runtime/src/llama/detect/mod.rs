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
/// This is the strict path used by callers that want to fail fast (for
/// example when the user explicitly opted in to a GPU build). For
/// install flows that should degrade gracefully, prefer
/// [`detect_optimal_acceleration_with_diagnostics`].
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

/// Detect the optimal acceleration type, downgrading to CPU when GPU
/// build dependencies are incomplete.
///
/// Returns the chosen [`Acceleration`] and a vector of human-readable
/// diagnostic warnings explaining any downgrade. The warnings are
/// ready to print directly to the user — they include the missing
/// component name and per-distro install hints.
///
/// Use this for auto-detect paths (e.g. `gglib config llama install`
/// without `--vulkan`) so a single missing package like SPIR-V headers
/// does not abort the install entirely; the caller can still build a
/// CPU-only `llama-server` and surface the warnings so the user
/// understands *why* GPU acceleration was skipped.
///
/// For the strict (fail-fast) variant used when the user explicitly
/// requested a GPU backend, see [`detect_optimal_acceleration`].
pub fn detect_optimal_acceleration_with_diagnostics() -> (Acceleration, Vec<String>) {
    if cfg!(target_os = "macos") && metal::has_metal_support() {
        return (Acceleration::Metal, Vec::new());
    }
    if cuda::has_cuda_toolkit() {
        return (Acceleration::Cuda, Vec::new());
    }

    let vk = vulkan_status();
    if vk.ready_for_build() {
        return (Acceleration::Vulkan, Vec::new());
    }

    // No GPU is fully buildable. Build a clear, actionable warning so
    // the CPU fallback isn't silent.
    let mut warnings = Vec::new();
    if vk.has_loader && !vk.missing.is_empty() {
        let labels: Vec<&str> = vk.missing.iter().map(|p| p.label()).collect();
        warnings.push(format!(
            "Vulkan runtime detected, but build dependencies are missing: {}.\n\
             Falling back to a CPU-only build.",
            labels.join(", ")
        ));
        for pkg in &vk.missing {
            let mut block = format!("To enable Vulkan, install {}:", pkg.label());
            for (distro, cmd) in pkg.install_hints() {
                block.push_str(&format!("\n  {distro:16} {cmd}"));
            }
            warnings.push(block);
        }
    } else {
        warnings.push(
            "No supported GPU acceleration found (Metal, CUDA, or Vulkan).\n\
             Falling back to a CPU-only build."
                .to_string(),
        );
    }

    (Acceleration::Cpu, warnings)
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

    #[test]
    fn test_detect_with_diagnostics_returns_acceleration_and_warnings() {
        // Smoke test: function never panics and contract holds —
        // either we got a buildable GPU backend (no warnings), or
        // we fell back to CPU with at least one explanatory warning.
        let (accel, warnings) = detect_optimal_acceleration_with_diagnostics();
        match accel {
            Acceleration::Metal | Acceleration::Cuda | Acceleration::Vulkan => {
                assert!(
                    warnings.is_empty(),
                    "GPU backends should not emit fallback warnings"
                );
            }
            Acceleration::Cpu => {
                assert!(
                    !warnings.is_empty(),
                    "CPU fallback must always include a diagnostic warning"
                );
                let combined = warnings.join("\n");
                assert!(
                    combined.contains("CPU"),
                    "fallback warning must mention CPU so the user understands the downgrade: {combined}"
                );
            }
        }
    }

    #[test]
    fn test_detect_with_diagnostics_warning_names_missing_component() {
        // When CPU is selected and Vulkan loader is present, the
        // warning must name at least one specific missing component
        // so the user knows what to install.
        let (accel, warnings) = detect_optimal_acceleration_with_diagnostics();
        if accel == Acceleration::Cpu && vulkan_status().has_loader {
            let combined = warnings.join("\n");
            let mentions_specific = combined.contains("SPIR-V")
                || combined.contains("Vulkan development headers")
                || combined.contains("glslc")
                || combined.contains("Vulkan loader");
            assert!(
                mentions_specific,
                "CPU-fallback warning must name the missing Vulkan component: {combined}"
            );
        }
    }
}
