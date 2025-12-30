//! System probe implementation for gglib-runtime.
//!
//! This module provides the `DefaultSystemProbe` which implements
//! `SystemProbePort` from gglib-core. It performs active system probing
//! via command execution and hardware detection.

mod commands;
mod deps;
mod gpu;

use gglib_core::ports::SystemProbePort;
use gglib_core::utils::system::{Dependency, DependencyStatus, GpuInfo, SystemMemoryInfo};

#[cfg(target_os = "linux")]
use commands::get_patchelf_version;
use commands::{
    get_cargo_version, get_cmake_version, get_gcc_version, get_git_version, get_gxx_version,
    get_make_version, get_node_version, get_npm_version, get_pkgconfig_version, get_rustc_version,
};
use deps::check_libssl;
#[cfg(target_os = "linux")]
use deps::{check_libappindicator, check_librsvg, check_webkit2gtk};
use gpu::{detect_gpu_info, get_system_memory_info};

/// Default implementation of `SystemProbePort`.
///
/// This struct provides active system probing by executing commands
/// and querying hardware. It should be constructed in CLI's main.rs
/// and passed to handlers that need system information.
///
/// # Example
///
/// ```ignore
/// use gglib_runtime::system::DefaultSystemProbe;
/// use gglib_core::ports::SystemProbePort;
///
/// let probe = DefaultSystemProbe::new();
/// let deps = probe.check_all_dependencies();
/// ```
pub struct DefaultSystemProbe;

impl DefaultSystemProbe {
    /// Create a new default system probe.
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultSystemProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProbePort for DefaultSystemProbe {
    fn check_all_dependencies(&self) -> Vec<Dependency> {
        let gpu_info = self.detect_gpu_info();

        let mut deps = vec![
            // Core Rust toolchain (required)
            Dependency::required("cargo", "Required for building Rust code")
                .with_hint("https://rustup.rs")
                .with_status(
                    get_cargo_version()
                        .map(|v| DependencyStatus::Present { version: v })
                        .unwrap_or(DependencyStatus::Missing),
                ),
            Dependency::required("rustc", "Rust compiler")
                .with_hint("https://rustup.rs")
                .with_status(
                    get_rustc_version()
                        .map(|v| DependencyStatus::Present { version: v })
                        .unwrap_or(DependencyStatus::Missing),
                ),
            // Node.js ecosystem (required for GUI)
            Dependency::required("node", "Required for building web UI and Tauri")
                .with_hint("https://nodejs.org")
                .with_status(
                    get_node_version()
                        .map(|v| DependencyStatus::Present { version: v })
                        .unwrap_or(DependencyStatus::Missing),
                ),
            Dependency::required("npm", "Node package manager")
                .with_hint("https://nodejs.org")
                .with_status(
                    get_npm_version()
                        .map(|v| DependencyStatus::Present { version: v })
                        .unwrap_or(DependencyStatus::Missing),
                ),
            // Build tools (required)
            Dependency::required("git", "Required for llama.cpp installation")
                .with_hint("apt install git")
                .with_status(
                    get_git_version()
                        .map(|v| DependencyStatus::Present { version: v })
                        .unwrap_or(DependencyStatus::Missing),
                ),
            Dependency::required("make", "Required for llama.cpp build")
                .with_hint("apt install build-essential")
                .with_status(
                    get_make_version()
                        .map(|v| DependencyStatus::Present { version: v })
                        .unwrap_or(DependencyStatus::Missing),
                ),
            Dependency::required("gcc", "Required for llama.cpp compilation")
                .with_hint("apt install build-essential")
                .with_status(
                    get_gcc_version()
                        .map(|v| DependencyStatus::Present { version: v })
                        .unwrap_or(DependencyStatus::Missing),
                ),
            Dependency::required("g++", "Required for llama.cpp compilation")
                .with_hint("apt install build-essential")
                .with_status(
                    get_gxx_version()
                        .map(|v| DependencyStatus::Present { version: v })
                        .unwrap_or(DependencyStatus::Missing),
                ),
            Dependency::required("pkg-config", "Required for building with system libraries")
                .with_hint("apt install pkg-config")
                .with_status(
                    get_pkgconfig_version()
                        .map(|v| DependencyStatus::Present { version: v })
                        .unwrap_or(DependencyStatus::Missing),
                ),
            Dependency::required("libssl-dev", "Required for HTTPS support")
                .with_hint("apt install libssl-dev")
                .with_status(
                    check_libssl()
                        .map(|v| DependencyStatus::Present { version: v })
                        .unwrap_or(DependencyStatus::Missing),
                ),
            Dependency::required("cmake", "Required for llama.cpp build")
                .with_hint("apt install cmake")
                .with_status(
                    get_cmake_version()
                        .map(|v| DependencyStatus::Present { version: v })
                        .unwrap_or(DependencyStatus::Missing),
                ),
        ];

        // Add GTK/Tauri dependencies for Linux only
        #[cfg(target_os = "linux")]
        {
            deps.extend(vec![
                Dependency::required("patchelf", "Required for Tauri AppImage bundling")
                    .with_hint("apt install patchelf")
                    .with_status(
                        get_patchelf_version()
                            .map(|v| DependencyStatus::Present { version: v })
                            .unwrap_or(DependencyStatus::Missing),
                    ),
                Dependency::required("webkit2gtk-4.1", "Required for Tauri desktop app (WebView)")
                    .with_hint("apt install libwebkit2gtk-4.1-dev")
                    .with_status(
                        check_webkit2gtk()
                            .map(|v| DependencyStatus::Present { version: v })
                            .unwrap_or(DependencyStatus::Missing),
                    ),
                Dependency::required("librsvg", "Required for Tauri desktop app (SVG rendering)")
                    .with_hint("apt install librsvg2-dev")
                    .with_status(
                        check_librsvg()
                            .map(|v| DependencyStatus::Present { version: v })
                            .unwrap_or(DependencyStatus::Missing),
                    ),
                Dependency::required(
                    "libappindicator-gtk3",
                    "Required for Tauri system tray support",
                )
                .with_hint("apt install libayatana-appindicator3-dev")
                .with_status(
                    check_libappindicator()
                        .map(|v| DependencyStatus::Present { version: v })
                        .unwrap_or(DependencyStatus::Missing),
                ),
            ]);
        }

        // Add GPU acceleration info based on detection
        if gpu_info.has_metal {
            deps.push(
                Dependency::optional("Metal", "Apple GPU acceleration (built-in)").with_status(
                    DependencyStatus::Present {
                        version: "available".to_string(),
                    },
                ),
            );
        } else if let Some(cuda_version) = gpu_info.cuda_version.clone() {
            deps.push(
                Dependency::optional("CUDA", "NVIDIA GPU acceleration for faster inference")
                    .with_hint("https://developer.nvidia.com/cuda-downloads")
                    .with_status(DependencyStatus::Present {
                        version: cuda_version,
                    }),
            );
        } else if gpu_info.has_nvidia_gpu {
            // GPU hardware present but CUDA not installed
            deps.push(
                Dependency::optional(
                    "CUDA",
                    "NVIDIA GPU detected - install CUDA for GPU acceleration",
                )
                .with_hint("https://developer.nvidia.com/cuda-downloads")
                .with_status(DependencyStatus::Optional),
            );
        }

        deps
    }

    fn detect_gpu_info(&self) -> GpuInfo {
        detect_gpu_info()
    }

    fn get_system_memory_info(&self) -> SystemMemoryInfo {
        get_system_memory_info()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_system_probe_creation() {
        let probe = DefaultSystemProbe::new();
        // Just verify it can be created
        let _deps = probe.check_all_dependencies();
    }

    #[test]
    fn test_default_system_probe_default_trait() {
        let probe = DefaultSystemProbe;
        let _deps = probe.check_all_dependencies();
    }

    #[test]
    fn test_gpu_detection() {
        let probe = DefaultSystemProbe::new();
        let _gpu = probe.detect_gpu_info();
        // On macOS, should have Metal
        #[cfg(target_os = "macos")]
        assert!(_gpu.has_metal);
    }

    #[test]
    fn test_memory_info() {
        let probe = DefaultSystemProbe::new();
        let mem = probe.get_system_memory_info();
        // RAM should always be > 1GB
        assert!(mem.total_ram_bytes > 1_000_000_000);
    }
}
