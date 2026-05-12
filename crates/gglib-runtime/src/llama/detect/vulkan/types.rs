//! Vulkan detection types — `MissingPackage` and `VulkanStatus`.
//!
//! These types are compiled on all platforms. On macOS,
//! [`VulkanStatus::absent`] returns the canonical all-false value that
//! signals Vulkan is not applicable (Metal is the native GPU API).

use serde::{Deserialize, Serialize};

// ============================================================================
// Types
// ============================================================================

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

    /// Returns a [`VulkanStatus`] where every component is absent.
    ///
    /// Used on platforms where Vulkan is not applicable (e.g. macOS, where
    /// Metal is the native GPU API). This is not an error — it is the
    /// canonical representation of "Vulkan does not apply here".
    pub fn absent() -> Self {
        VulkanStatus {
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
        }
    }
}

// ============================================================================
// Tests — compiled on all platforms including macOS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_vulkan_status_absent_is_not_ready() {
        assert!(!VulkanStatus::absent().ready_for_build());
    }

    #[test]
    fn test_vulkan_status_absent_lists_all_missing() {
        let s = VulkanStatus::absent();
        assert_eq!(s.missing.len(), 4);
        assert!(s.missing.contains(&MissingPackage::VulkanLoader));
        assert!(s.missing.contains(&MissingPackage::VulkanHeaders));
        assert!(s.missing.contains(&MissingPackage::Glslc));
        assert!(s.missing.contains(&MissingPackage::SpirvHeaders));
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
            missing: vec![MissingPackage::VulkanHeaders, MissingPackage::SpirvHeaders],
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
