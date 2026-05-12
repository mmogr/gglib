//! Vulkan component probing — Linux and Windows only.
//!
//! This file is **not compiled on macOS**. It is declared in `mod.rs` only
//! when `target_os` is `linux` or `windows`:
//!
//! ```text
//! #[cfg(any(target_os = "linux", target_os = "windows"))]
//! mod probe;
//! ```
//!
//! On macOS [`super::vulkan_status`] returns [`super::VulkanStatus::absent`]
//! directly without touching any of the probing logic here.

use std::path::{Path, PathBuf};

use super::super::tools::{command_stdout, command_succeeds};
use super::types::MissingPackage;
use super::types::VulkanStatus;

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
fn vulkan_sdk_dir() -> Option<PathBuf> {
    std::env::var_os("VULKAN_SDK").map(PathBuf::from)
}

// ============================================================================
// Component checks
// ============================================================================

/// Check whether a Vulkan loader is available on the system.
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
        let roots: &[&Path] = &[Path::new("/usr/include"), Path::new("/usr/local/include")];
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
        let roots: &[&Path] = &[Path::new("/usr/include"), Path::new("/usr/local/include")];
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

// ============================================================================
// Public entry point (visible only within the vulkan module)
// ============================================================================

/// Probe the system for Vulkan build-readiness.
///
/// Only called on Linux and Windows — `mod.rs` dispatches here only when
/// `target_os` is one of those two platforms.
pub(super) fn vulkan_status() -> VulkanStatus {
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

// ============================================================================
// Tests — compiled only on Linux/Windows (by virtue of module gating)
// ============================================================================

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
}
