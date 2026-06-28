#![doc = include_str!("README.md")]

// MIGRATION: content extracted to README.md — remove this //! block after review
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
//! # Platform scope
//!
//! Vulkan probing ([`probe`]) is compiled only on Linux and Windows.
//! On macOS, [`vulkan_status`] returns [`VulkanStatus::absent`] — not
//! an error, but the canonical signal that Metal is the native GPU API
//! and Vulkan is not applicable on this platform.
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

mod types;

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod probe;

pub use types::{MissingPackage, VulkanStatus};

/// Probe the system for Vulkan build-readiness.
///
/// On Linux and Windows, checks for the Vulkan loader, development
/// headers, SPIR-V headers, and the `glslc` shader compiler.
///
/// On all other platforms (e.g. macOS, which uses Metal), returns
/// [`VulkanStatus::absent`] immediately — this is not an error.
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn vulkan_status() -> VulkanStatus {
    probe::vulkan_status()
}

/// Probe the system for Vulkan build-readiness.
///
/// On Linux and Windows, checks for the Vulkan loader, development
/// headers, SPIR-V headers, and the `glslc` shader compiler.
///
/// On all other platforms (e.g. macOS, which uses Metal), returns
/// [`VulkanStatus::absent`] immediately — this is not an error.
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn vulkan_status() -> VulkanStatus {
    VulkanStatus::absent()
}
