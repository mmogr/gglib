#![doc = include_str!("README.md")]
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
