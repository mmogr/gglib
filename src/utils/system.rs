//! System utility functions for checking dependencies and environment.
//!
//! This module provides cross-platform utilities for checking system
//! dependencies, command availability, and version information.

use serde::{Deserialize, Serialize};
use std::process::Command;
use sysinfo::System;

/// Represents the status of a system dependency
#[derive(Debug, Clone, PartialEq)]
pub enum DependencyStatus {
    /// Dependency is installed and available
    Present { version: String },
    /// Dependency is missing
    Missing,
    /// Dependency is optional (not required for basic functionality)
    Optional,
}

/// Information about a system dependency
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Name of the dependency (e.g., "cargo", "node")
    pub name: String,
    /// Current status of the dependency
    pub status: DependencyStatus,
    /// Description of what this dependency is used for
    pub description: String,
    /// Whether this dependency is required or optional
    pub required: bool,
    /// Installation instructions or hints
    pub install_hint: Option<String>,
}

impl Dependency {
    /// Create a new required dependency
    pub fn required(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DependencyStatus::Missing,
            description: description.into(),
            required: true,
            install_hint: None,
        }
    }

    /// Create a new optional dependency
    pub fn optional(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: DependencyStatus::Optional,
            description: description.into(),
            required: false,
            install_hint: None,
        }
    }

    /// Set installation hint
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.install_hint = Some(hint.into());
        self
    }

    /// Set the status of this dependency
    pub fn with_status(mut self, status: DependencyStatus) -> Self {
        self.status = status;
        self
    }
}

/// Check if a command exists in the system PATH
pub fn command_exists(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Get version of a command by running it with --version
pub fn get_command_version(cmd: &str) -> Option<String> {
    let output = Command::new(cmd).arg("--version").output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout
        .lines()
        .next()?
        .split_whitespace()
        .find(|s| s.chars().next().map(|c| c.is_numeric()).unwrap_or(false))?
        .to_string();

    Some(version)
}

/// Get version of cargo specifically
pub fn get_cargo_version() -> Option<String> {
    let output = Command::new("cargo").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.split_whitespace().nth(1).map(|s| s.to_string())
}

/// Get version of rustc specifically
pub fn get_rustc_version() -> Option<String> {
    let output = Command::new("rustc").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.split_whitespace().nth(1).map(|s| s.to_string())
}

/// Get version of node
pub fn get_node_version() -> Option<String> {
    let output = Command::new("node").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string()
        .into()
}

/// Get version of npm
pub fn get_npm_version() -> Option<String> {
    let output = Command::new("npm").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(format!(
        "v{}",
        String::from_utf8_lossy(&output.stdout).trim()
    ))
}

/// Get version of git
pub fn get_git_version() -> Option<String> {
    let output = Command::new("git").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.split_whitespace().nth(2).map(|s| s.to_string())
}

/// Get version of cmake
pub fn get_cmake_version() -> Option<String> {
    let output = Command::new("cmake").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()?
        .split_whitespace()
        .nth(2)
        .map(|s| s.to_string())
}

/// Get version of make
pub fn get_make_version() -> Option<String> {
    let output = Command::new("make").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()?
        .split_whitespace()
        .nth(2)
        .map(|s| s.to_string())
}

/// Get version of gcc
pub fn get_gcc_version() -> Option<String> {
    let output = Command::new("gcc").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()?
        .split_whitespace()
        .last()
        .map(|s| s.to_string())
}

/// Get version of g++
pub fn get_gxx_version() -> Option<String> {
    let output = Command::new("g++").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()?
        .split_whitespace()
        .last()
        .map(|s| s.to_string())
}

/// Get version of pkg-config
pub fn get_pkgconfig_version() -> Option<String> {
    let output = Command::new("pkg-config").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string()
        .into()
}

/// Get version of patchelf
pub fn get_patchelf_version() -> Option<String> {
    let output = Command::new("patchelf").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.split_whitespace().nth(1).map(|s| s.to_string())
}

/// Check if libssl-dev is installed by checking for OpenSSL with pkg-config
pub fn check_libssl() -> Option<String> {
    let output = Command::new("pkg-config")
        .args(["--modversion", "openssl"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string()
        .into()
}

/// Check for a library using pkg-config
fn check_pkg_config_lib(lib_name: &str) -> Option<String> {
    let output = Command::new("pkg-config")
        .args(["--modversion", lib_name])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string()
        .into()
}

/// Check if webkit2gtk is installed (tries 4.1 then falls back to 4.0)
pub fn check_webkit2gtk() -> Option<String> {
    // Try webkit2gtk-4.1 first (Ubuntu 24.04+)
    if let Some(version) = check_pkg_config_lib("webkit2gtk-4.1") {
        return Some(version);
    }
    // Fall back to webkit2gtk-4.0 (older versions)
    check_pkg_config_lib("webkit2gtk-4.0")
}

/// Check if librsvg is installed
pub fn check_librsvg() -> Option<String> {
    check_pkg_config_lib("librsvg-2.0")
}

/// Check if libappindicator-gtk3 is installed
pub fn check_libappindicator() -> Option<String> {
    // Try ayatana-appindicator first (newer Ubuntu/Debian)
    if let Some(version) = check_pkg_config_lib("ayatana-appindicator3-0.1") {
        return Some(version);
    }
    // Fall back to older appindicator
    check_pkg_config_lib("appindicator3-0.1")
}

/// GPU hardware detection result
#[derive(Debug, Clone, PartialEq)]
pub struct GpuInfo {
    /// NVIDIA GPU hardware detected (via nvidia-smi, lspci, etc.)
    pub has_nvidia_gpu: bool,
    /// CUDA toolkit installed and available
    pub cuda_version: Option<String>,
    /// On macOS (Metal always available)
    pub has_metal: bool,
}

/// System memory information for model fit calculations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMemoryInfo {
    /// Total system RAM in bytes
    pub total_ram_bytes: u64,
    /// GPU memory in bytes (VRAM for discrete GPUs, or unified memory portion for Apple Silicon)
    /// None if no GPU detected or memory couldn't be determined
    pub gpu_memory_bytes: Option<u64>,
    /// Whether the system has Apple Silicon with unified memory
    pub is_apple_silicon: bool,
    /// Whether the system has an NVIDIA GPU
    pub has_nvidia_gpu: bool,
}

/// Get system memory information for model fit calculations
pub fn get_system_memory_info() -> SystemMemoryInfo {
    let sys = System::new_all();
    let total_ram_bytes = sys.total_memory();
    let gpu_info = detect_gpu_info();

    let (gpu_memory_bytes, is_apple_silicon) = if gpu_info.has_metal {
        // Apple Silicon: unified memory architecture
        // Approximately 75% of total RAM is available for GPU use
        let gpu_available = (total_ram_bytes as f64 * 0.75) as u64;
        (Some(gpu_available), true)
    } else if gpu_info.has_nvidia_gpu {
        // NVIDIA GPU: query VRAM via nvidia-smi
        let vram = get_nvidia_vram_bytes();
        (vram, false)
    } else {
        // No GPU acceleration available, will use RAM
        (None, false)
    };

    SystemMemoryInfo {
        total_ram_bytes,
        gpu_memory_bytes,
        is_apple_silicon,
        has_nvidia_gpu: gpu_info.has_nvidia_gpu,
    }
}

/// Get NVIDIA GPU VRAM in bytes using nvidia-smi
fn get_nvidia_vram_bytes() -> Option<u64> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // nvidia-smi returns memory in MiB, convert to bytes
    // If multiple GPUs, take the first one
    let mib: u64 = stdout.lines().next()?.trim().parse().ok()?;
    Some(mib * 1024 * 1024)
}

/// Detect GPU hardware and acceleration software
pub fn detect_gpu_info() -> GpuInfo {
    let has_metal = cfg!(target_os = "macos");
    let has_nvidia_gpu = detect_nvidia_hardware();
    let cuda_version = check_cuda();

    GpuInfo {
        has_nvidia_gpu,
        cuda_version,
        has_metal,
    }
}

/// Detect if NVIDIA GPU hardware is present (regardless of CUDA installation)
fn detect_nvidia_hardware() -> bool {
    // Try nvidia-smi (most reliable if NVIDIA drivers are installed)
    if Command::new("nvidia-smi")
        .arg("--list-gpus")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }

    // Try lspci on Linux
    #[cfg(target_os = "linux")]
    {
        if Command::new("lspci")
            .output()
            .map(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout)
                        .to_lowercase()
                        .contains("nvidia")
            })
            .unwrap_or(false)
        {
            return true;
        }
    }

    // Try wmic on Windows
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = Command::new("wmic")
            .args(["path", "win32_VideoController", "get", "name"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.to_lowercase().contains("nvidia") {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if NVIDIA CUDA toolkit is installed
#[allow(clippy::collapsible_if)]
fn check_cuda() -> Option<String> {
    // Try nvcc first (CUDA compiler)
    if let Ok(output) = Command::new("nvcc").arg("--version").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Extract version from "Cuda compilation tools, release 12.0, V12.0.140"
            if let Some(line) = stdout.lines().find(|l| l.contains("release")) {
                if let Some(version) = line.split("release").nth(1) {
                    let version = version.trim().split(',').next().unwrap_or("").trim();
                    if !version.is_empty() {
                        return Some(version.to_string());
                    }
                }
            }
        }
    }

    None
}

/// Parse a version string into a tuple of (major, minor)
///
/// Examples: "13.0" -> Some((13, 0)), "12.3.5" -> Some((12, 3))
/// Handles edge cases like "12.0-rc1" -> Some((12, 0)), "12.0.0ubuntu1" -> Some((12, 0))
///
/// Returns None if:
/// - The version string has fewer than 2 dot-separated components
/// - Either the major or minor component doesn't start with a digit
/// - The numeric portion of major/minor cannot be parsed as u32
///
/// Note: This function extracts only the leading numeric portion of each component,
/// so "12.0-rc1" successfully parses as (12, 0) by ignoring the "-rc1" suffix.
pub fn parse_version_tuple(version_str: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() >= 2 {
        // Helper to extract numeric portion from a version component
        let parse_numeric = |part: &str| -> Option<u32> {
            let numeric_str: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            numeric_str.parse::<u32>().ok()
        };

        let major = parse_numeric(parts[0])?;
        let minor = parse_numeric(parts[1])?;
        Some((major, minor))
    } else {
        None
    }
}

/// Get CUDA version as a tuple (major, minor)
/// Returns None if CUDA is not installed or version cannot be determined
pub fn get_cuda_version_tuple() -> Option<(u32, u32)> {
    let version_str = check_cuda()?;
    parse_version_tuple(&version_str)
}

/// Get GCC version as a tuple (major, minor)
/// Returns None if GCC is not installed or version cannot be determined
pub fn get_gcc_version_tuple() -> Option<(u32, u32)> {
    let version_str = get_gcc_version()?;
    parse_version_tuple(&version_str)
}

/// Check all system dependencies and return their status
pub fn check_all_dependencies() -> Vec<Dependency> {
    let gpu_info = detect_gpu_info();

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
    } else if let Some(cuda_version) = gpu_info.cuda_version {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_exists() {
        // Test with commands that support --version on all platforms
        // cargo should exist since we're running tests with cargo
        assert!(command_exists("cargo"));
    }

    #[test]
    fn test_dependency_creation() {
        let dep = Dependency::required("test", "Test dependency");
        assert_eq!(dep.name, "test");
        assert_eq!(dep.description, "Test dependency");
        assert!(dep.required);
    }

    #[test]
    fn test_dependency_with_hint() {
        let dep = Dependency::required("test", "Test dependency").with_hint("test install command");
        assert_eq!(dep.install_hint, Some("test install command".to_string()));
    }

    #[test]
    fn test_get_system_memory_info() {
        let info = get_system_memory_info();
        // RAM should always be detected and be a reasonable value (> 1GB)
        assert!(info.total_ram_bytes > 1_000_000_000);

        // On macOS, we should detect Apple Silicon unified memory
        #[cfg(target_os = "macos")]
        {
            assert!(info.is_apple_silicon);
            assert!(info.gpu_memory_bytes.is_some());
            // GPU memory should be ~75% of RAM
            let expected_gpu = (info.total_ram_bytes as f64 * 0.75) as u64;
            assert_eq!(info.gpu_memory_bytes, Some(expected_gpu));
        }
    }

    #[test]
    fn test_system_memory_info_serialization() {
        let info = SystemMemoryInfo {
            total_ram_bytes: 16_000_000_000,
            gpu_memory_bytes: Some(12_000_000_000),
            is_apple_silicon: true,
            has_nvidia_gpu: false,
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: SystemMemoryInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.total_ram_bytes, info.total_ram_bytes);
        assert_eq!(parsed.gpu_memory_bytes, info.gpu_memory_bytes);
        assert_eq!(parsed.is_apple_silicon, info.is_apple_silicon);
        assert_eq!(parsed.has_nvidia_gpu, info.has_nvidia_gpu);
    }
}
