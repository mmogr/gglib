//! Vulkan acceleration detection with build-readiness validation.
//!
//! Vulkan is a portable GPU API supported on AMD, Intel, and NVIDIA
//! hardware across Linux and Windows. Building llama.cpp with
//! `-DGGML_VULKAN=ON` requires three things beyond the runtime:
//!
//! 1. **Vulkan loader** — `libvulkan.so.1` (Linux) or `vulkan-1.dll`
//!    (Windows), confirmed by `vulkaninfo --summary`.
//! 2. **Vulkan development headers** — `vulkan/vulkan.h`, needed by
//!    CMake's `FindVulkan.cmake` to set `Vulkan_INCLUDE_DIR`.
//! 3. **SPIR-V shader compiler** — `glslc`, used to compile Vulkan
//!    compute shaders at build time.
//!
//! [`VulkanStatus`] captures all three independently so callers can
//! give precise, actionable diagnostics when a build fails the
//! pre-flight check.
//!
//! # Why this matters
//!
//! Many Linux distributions ship Vulkan *runtime* libraries by default
//! (Mesa drivers, libvulkan), but **not** the development headers or
//! shader compiler. A system that passes `vulkaninfo --summary` can
//! still fail CMake's `FindVulkan` with:
//!
//! ```text
//! Could NOT find Vulkan (missing: Vulkan_INCLUDE_DIR)
//! ```
//!
//! This module's [`vulkan_status`] function detects the gap *before*
//! invoking CMake, allowing the CLI and GUI to surface distro-specific
//! install instructions.

use serde::{Deserialize, Serialize};

use super::tools::command_succeeds;

/// A component required for a Vulkan-accelerated build that is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MissingPackage {
    /// Vulkan loader library (`libvulkan.so.1` / `vulkan-1.dll`).
    VulkanLoader,
    /// Vulkan development headers (`vulkan/vulkan.h`).
    VulkanHeaders,
    /// SPIR-V shader compiler (`glslc`).
    Glslc,
}

impl MissingPackage {
    /// Return a human-readable label for this missing component.
    pub fn label(&self) -> &str {
        match self {
            MissingPackage::VulkanLoader => "Vulkan loader (libvulkan)",
            MissingPackage::VulkanHeaders => "Vulkan development headers",
            MissingPackage::Glslc => "SPIR-V shader compiler (glslc)",
        }
    }

    /// Return distro-specific install hints for this missing component.
    ///
    /// Returns a list of `(distro_label, command)` pairs.
    pub fn install_hints(&self) -> Vec<(&str, &str)> {
        match self {
            MissingPackage::VulkanLoader => vec![
                ("Arch", "sudo pacman -S vulkan-icd-loader"),
                ("Ubuntu/Debian", "sudo apt install libvulkan1"),
                ("Fedora", "sudo dnf install vulkan-loader"),
            ],
            MissingPackage::VulkanHeaders => vec![
                ("Arch", "sudo pacman -S vulkan-headers"),
                ("Ubuntu/Debian", "sudo apt install libvulkan-dev"),
                ("Fedora", "sudo dnf install vulkan-devel"),
            ],
            MissingPackage::Glslc => vec![
                ("Arch", "sudo pacman -S shaderc"),
                ("Ubuntu/Debian", "sudo apt install glslc"),
                ("Fedora", "sudo dnf install glslc"),
            ],
        }
    }
}

/// Comprehensive Vulkan build-readiness status.
///
/// Captures the presence of each component independently so callers
/// can provide targeted remediation advice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VulkanStatus {
    /// Vulkan runtime loader is present and working.
    pub has_loader: bool,
    /// Vulkan development headers are installed.
    pub has_headers: bool,
    /// The `glslc` SPIR-V shader compiler is available.
    pub has_glslc: bool,
    /// Components that are missing (empty if fully ready).
    pub missing: Vec<MissingPackage>,
}

impl VulkanStatus {
    /// Returns `true` if all three components needed for a
    /// `-DGGML_VULKAN=ON` build are present.
    pub fn ready_for_build(&self) -> bool {
        self.has_loader && self.has_headers && self.has_glslc
    }
}

/// Probe the system for Vulkan build-readiness.
///
/// Checks for the Vulkan loader, development headers, and the `glslc`
/// shader compiler. macOS always returns a fully-absent status because
/// it uses Metal instead of Vulkan.
pub fn vulkan_status() -> VulkanStatus {
    if cfg!(target_os = "macos") {
        return VulkanStatus {
            has_loader: false,
            has_headers: false,
            has_glslc: false,
            missing: vec![
                MissingPackage::VulkanLoader,
                MissingPackage::VulkanHeaders,
                MissingPackage::Glslc,
            ],
        };
    }

    let has_loader = check_vulkan_loader();
    let has_headers = check_vulkan_headers();
    let has_glslc = command_succeeds("glslc", &["--version"]);

    let mut missing = Vec::new();
    if !has_loader {
        missing.push(MissingPackage::VulkanLoader);
    }
    if !has_headers {
        missing.push(MissingPackage::VulkanHeaders);
    }
    if !has_glslc {
        missing.push(MissingPackage::Glslc);
    }

    VulkanStatus {
        has_loader,
        has_headers,
        has_glslc,
        missing,
    }
}

/// Check whether a Vulkan loader is available on the system.
///
/// This is the runtime-only check used by
/// [`detect_optimal_acceleration`](super::detect_optimal_acceleration)
/// to decide whether Vulkan is a candidate.
fn check_vulkan_loader() -> bool {
    // vulkaninfo is the most reliable indicator
    if command_succeeds("vulkaninfo", &["--summary"]) {
        return true;
    }

    // Fall back to checking for the loader library on disk
    #[cfg(target_os = "linux")]
    {
        use std::path::Path;
        if Path::new("/usr/lib/x86_64-linux-gnu/libvulkan.so.1").exists()
            || Path::new("/usr/lib64/libvulkan.so.1").exists()
            || Path::new("/usr/lib/libvulkan.so.1").exists()
        {
            return true;
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(sys_root) = std::env::var("SystemRoot") {
            let vulkan_dll = std::path::Path::new(&sys_root)
                .join("System32")
                .join("vulkan-1.dll");
            if vulkan_dll.exists() {
                return true;
            }
        }
    }

    false
}

/// Check whether Vulkan development headers are installed.
///
/// Checks for the actual `vulkan/vulkan.h` header file that CMake's
/// `FindVulkan.cmake` needs to set `Vulkan_INCLUDE_DIR`. We cannot
/// rely on `pkg-config --exists vulkan` alone because on some distros
/// (e.g. Arch) the `.pc` file ships with the runtime loader package,
/// not the headers package.
fn check_vulkan_headers() -> bool {
    use std::path::Path;

    // Direct header-file check — mirrors what CMake's FindVulkan does.
    #[cfg(target_os = "linux")]
    {
        if Path::new("/usr/include/vulkan/vulkan.h").exists()
            || Path::new("/usr/local/include/vulkan/vulkan.h").exists()
        {
            return true;
        }
    }

    // On other platforms, fall back to pkg-config with a cflags probe
    // that verifies the include directory actually contains the header.
    if command_succeeds("pkg-config", &["--exists", "vulkan"])
        && let Some(includedir) =
            super::tools::command_stdout("pkg-config", &["--variable=includedir", "vulkan"])
        && Path::new(includedir.trim())
            .join("vulkan/vulkan.h")
            .exists()
    {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vulkan_status_ready_when_all_present() {
        let status = VulkanStatus {
            has_loader: true,
            has_headers: true,
            has_glslc: true,
            missing: vec![],
        };
        assert!(status.ready_for_build());
    }

    #[test]
    fn test_vulkan_status_not_ready_without_headers() {
        let status = VulkanStatus {
            has_loader: true,
            has_headers: false,
            has_glslc: true,
            missing: vec![MissingPackage::VulkanHeaders],
        };
        assert!(!status.ready_for_build());
    }

    #[test]
    fn test_vulkan_status_not_ready_without_glslc() {
        let status = VulkanStatus {
            has_loader: true,
            has_headers: true,
            has_glslc: false,
            missing: vec![MissingPackage::Glslc],
        };
        assert!(!status.ready_for_build());
    }

    #[test]
    fn test_missing_package_labels() {
        assert!(!MissingPackage::VulkanLoader.label().is_empty());
        assert!(!MissingPackage::VulkanHeaders.label().is_empty());
        assert!(!MissingPackage::Glslc.label().is_empty());
    }

    #[test]
    fn test_missing_package_install_hints_non_empty() {
        assert!(!MissingPackage::VulkanHeaders.install_hints().is_empty());
        assert!(!MissingPackage::Glslc.install_hints().is_empty());
    }

    #[test]
    fn test_vulkan_status_serialization() {
        let status = VulkanStatus {
            has_loader: true,
            has_headers: false,
            has_glslc: true,
            missing: vec![MissingPackage::VulkanHeaders],
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("hasLoader"));
        assert!(json.contains("hasHeaders"));
        assert!(json.contains("hasGlslc"));
        assert!(json.contains("vulkanHeaders"));
    }
}
