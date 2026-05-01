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

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::tools::{command_stdout, command_succeeds};

// ============================================================================
// Probe helpers (unit-testable, dependency-injection friendly)
// ============================================================================

/// Return `true` if `<root>/<rel>` exists for any `root` in `roots`.
///
/// Pure function over filesystem state — used by the header probes so that
/// tests can construct fake filesystem layouts in `tempfile::TempDir`s
/// without touching the developer's real `/usr/include`.
fn header_exists_in(roots: &[&Path], rel: &str) -> bool {
    roots.iter().any(|root| root.join(rel).exists())
}

/// Query `pkg-config --variable=includedir <pkg>` and return the result
/// as a `PathBuf` if the package is known and the value is non-empty.
///
/// Returns `None` when pkg-config is missing, the package is unknown, or
/// the include directory string is empty/whitespace.
fn pkg_config_includedir(pkg: &str) -> Option<PathBuf> {
    if !command_succeeds("pkg-config", &["--exists", pkg]) {
        return None;
    }
    let raw = command_stdout("pkg-config", &["--variable=includedir", pkg])?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

/// Return the LunarG Vulkan SDK install root from the `VULKAN_SDK` env var.
///
/// Wrapping the env read in a function makes it trivial to swap with a
/// stub in tests so that a developer's local SDK never leaks into CI.
#[allow(dead_code)] // Used by Windows-only code paths and the SPIR-V probe (next commit).
fn vulkan_sdk_dir() -> Option<PathBuf> {
    std::env::var_os("VULKAN_SDK").map(PathBuf::from)
}

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
    /// SPIR-V headers (`spirv/unified1/spirv.hpp`).
    ///
    /// llama.cpp's `ggml-vulkan.cpp` includes this header directly. It is
    /// shipped as a separate package on every Linux distribution
    /// (independent of `vulkan-headers`) and is bundled inside the LunarG
    /// Vulkan SDK on Windows.
    SpirvHeaders,
}

impl MissingPackage {
    /// Return a human-readable label for this missing component.
    pub fn label(&self) -> &str {
        match self {
            MissingPackage::VulkanLoader => "Vulkan loader (libvulkan)",
            MissingPackage::VulkanHeaders => "Vulkan development headers",
            MissingPackage::Glslc => "SPIR-V shader compiler (glslc)",
            MissingPackage::SpirvHeaders => "SPIR-V headers (spirv-headers)",
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
            MissingPackage::SpirvHeaders => vec![
                ("Arch", "sudo pacman -S spirv-headers"),
                ("Ubuntu/Debian", "sudo apt install spirv-headers"),
                ("Fedora", "sudo dnf install spirv-headers-devel"),
                (
                    "Windows",
                    "Install the LunarG Vulkan SDK from https://vulkan.lunarg.com/sdk/home (bundles SPIRV-Headers)",
                ),
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
    /// SPIR-V headers (`spirv/unified1/spirv.hpp`) are installed.
    ///
    /// Required by llama.cpp's `ggml-vulkan.cpp` at build time and ships
    /// as a separate package from the Vulkan loader/headers on Linux.
    pub has_spirv_headers: bool,
    /// Components that are missing (empty if fully ready).
    pub missing: Vec<MissingPackage>,
}

impl VulkanStatus {
    /// Returns `true` if all components needed for a `-DGGML_VULKAN=ON`
    /// build are present.
    pub fn ready_for_build(&self) -> bool {
        self.has_loader && self.has_headers && self.has_glslc && self.has_spirv_headers
    }
}

/// Probe the system for Vulkan build-readiness.
///
/// Checks for the Vulkan loader, development headers, SPIR-V headers,
/// and the `glslc` shader compiler. macOS always returns a fully-absent
/// status because it uses Metal instead of Vulkan.
pub fn vulkan_status() -> VulkanStatus {
    if cfg!(target_os = "macos") {
        return VulkanStatus {
            has_loader: false,
            has_headers: false,
            has_glslc: false,
            has_spirv_headers: false,
            missing: vec![
                MissingPackage::VulkanLoader,
                MissingPackage::VulkanHeaders,
                MissingPackage::Glslc,
                MissingPackage::SpirvHeaders,
            ],
        };
    }

    let has_loader = check_vulkan_loader();
    let has_headers = check_vulkan_headers();
    let has_glslc = command_succeeds("glslc", &["--version"]);
    let has_spirv_headers = check_spirv_headers();

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
    if !has_spirv_headers {
        missing.push(MissingPackage::SpirvHeaders);
    }

    VulkanStatus {
        has_loader,
        has_headers,
        has_glslc,
        has_spirv_headers,
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
/// Probe order (uniform across platforms):
/// 1. `pkg-config --variable=includedir vulkan` → check
///    `<includedir>/vulkan/vulkan.h`. Works on NixOS, Homebrew, and any
///    distro that publishes a `.pc` file for the headers package.
/// 2. Hardcoded Linux paths (`/usr/include`, `/usr/local/include`).
/// 3. Hardcoded Windows path under `%VULKAN_SDK%/Include`.
///
/// We deliberately do not rely on `pkg-config --exists vulkan` alone —
/// on some distros (e.g. Arch) the `.pc` file ships with the runtime
/// loader package, not the headers package, so we always verify the
/// `vulkan.h` file is reachable from the include dir.
fn check_vulkan_headers() -> bool {
    const REL: &str = "vulkan/vulkan.h";

    // 1. pkg-config first — handles non-standard prefixes uniformly.
    if let Some(includedir) = pkg_config_includedir("vulkan")
        && includedir.join(REL).exists()
    {
        return true;
    }

    // 2. Hardcoded fallback paths per OS.
    #[cfg(target_os = "linux")]
    {
        let roots: &[&Path] = &[
            Path::new("/usr/include"),
            Path::new("/usr/local/include"),
        ];
        if header_exists_in(roots, REL) {
            return true;
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(sdk) = vulkan_sdk_dir() {
            let include = sdk.join("Include");
            if include.join(REL).exists() {
                return true;
            }
        }
    }

    false
}

/// Check whether SPIR-V headers (`spirv/unified1/spirv.hpp`) are installed.
///
/// llama.cpp's `ggml-vulkan.cpp` `#include`s `<spirv/unified1/spirv.hpp>`
/// (with `<spirv-headers/spirv.hpp>` as an `__has_include` fallback).
/// This is provided by the **separate** SPIRV-Headers package — distinct
/// from `vulkan-headers` on every Linux distribution. On Windows it ships
/// inside the LunarG Vulkan SDK.
///
/// Probe order (uniform across platforms):
/// 1. `pkg-config --variable=includedir SPIRV-Headers`.
/// 2. Hardcoded Linux paths (`/usr/include`, `/usr/local/include`).
/// 3. Hardcoded Windows path under `%VULKAN_SDK%/Include`.
fn check_spirv_headers() -> bool {
    // Both header layouts that ggml-vulkan.cpp accepts via __has_include.
    const REL_PRIMARY: &str = "spirv/unified1/spirv.hpp";
    const REL_ALTERNATE: &str = "spirv-headers/spirv.hpp";

    // 1. pkg-config first (works on NixOS, Homebrew, custom prefixes).
    if let Some(includedir) = pkg_config_includedir("SPIRV-Headers")
        && (includedir.join(REL_PRIMARY).exists() || includedir.join(REL_ALTERNATE).exists())
    {
        return true;
    }

    // 2. Hardcoded fallback paths per OS.
    #[cfg(target_os = "linux")]
    {
        let roots: &[&Path] = &[
            Path::new("/usr/include"),
            Path::new("/usr/local/include"),
        ];
        if header_exists_in(roots, REL_PRIMARY) || header_exists_in(roots, REL_ALTERNATE) {
            return true;
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(sdk) = vulkan_sdk_dir() {
            let include = sdk.join("Include");
            if include.join(REL_PRIMARY).exists() || include.join(REL_ALTERNATE).exists() {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_header_exists_in_finds_existing_header() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("vulkan");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("vulkan.h"), "// stub header").unwrap();

        assert!(header_exists_in(&[dir.path()], "vulkan/vulkan.h"));
    }

    #[test]
    fn test_header_exists_in_returns_false_when_absent() {
        let dir = TempDir::new().unwrap();
        assert!(!header_exists_in(&[dir.path()], "vulkan/vulkan.h"));
    }

    #[test]
    fn test_header_exists_in_searches_multiple_roots() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();
        let nested = dir2.path().join("foo");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("bar.h"), "").unwrap();

        // Only dir2 has the file, but the search covers both.
        assert!(header_exists_in(&[dir1.path(), dir2.path()], "foo/bar.h"));
    }

    #[test]
    fn test_vulkan_status_ready_when_all_present() {
        let status = VulkanStatus {
            has_loader: true,
            has_headers: true,
            has_glslc: true,
            has_spirv_headers: true,
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
            has_spirv_headers: true,
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
            has_spirv_headers: true,
            missing: vec![MissingPackage::Glslc],
        };
        assert!(!status.ready_for_build());
    }

    #[test]
    fn test_vulkan_status_not_ready_without_spirv_headers() {
        let status = VulkanStatus {
            has_loader: true,
            has_headers: true,
            has_glslc: true,
            has_spirv_headers: false,
            missing: vec![MissingPackage::SpirvHeaders],
        };
        assert!(!status.ready_for_build());
    }

    #[test]
    fn test_missing_package_labels() {
        assert!(!MissingPackage::VulkanLoader.label().is_empty());
        assert!(!MissingPackage::VulkanHeaders.label().is_empty());
        assert!(!MissingPackage::Glslc.label().is_empty());
        assert!(!MissingPackage::SpirvHeaders.label().is_empty());
    }

    #[test]
    fn test_missing_package_install_hints_non_empty() {
        assert!(!MissingPackage::VulkanLoader.install_hints().is_empty());
        assert!(!MissingPackage::VulkanHeaders.install_hints().is_empty());
        assert!(!MissingPackage::Glslc.install_hints().is_empty());
        assert!(!MissingPackage::SpirvHeaders.install_hints().is_empty());
    }

    #[test]
    fn test_spirv_headers_install_hints_mention_lunarg_on_windows() {
        // The Windows hint must steer users to the LunarG SDK installer
        // (no winget/choco package exists for SPIRV-Headers proper).
        let hints = MissingPackage::SpirvHeaders.install_hints();
        let windows_hint = hints
            .iter()
            .find(|(label, _)| *label == "Windows")
            .expect("Windows hint missing");
        assert!(
            windows_hint.1.to_lowercase().contains("lunarg"),
            "Windows SPIR-V hint must reference LunarG SDK, got: {}",
            windows_hint.1
        );
    }

    #[test]
    fn test_vulkan_status_serialization() {
        let status = VulkanStatus {
            has_loader: true,
            has_headers: false,
            has_glslc: true,
            has_spirv_headers: false,
            missing: vec![
                MissingPackage::VulkanHeaders,
                MissingPackage::SpirvHeaders,
            ],
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("hasLoader"));
        assert!(json.contains("hasHeaders"));
        assert!(json.contains("hasGlslc"));
        assert!(json.contains("hasSpirvHeaders"));
        assert!(json.contains("vulkanHeaders"));
        assert!(json.contains("spirvHeaders"));
    }
}
