//! Metal acceleration detection for macOS.
//!
//! Metal is Apple's GPU API available on macOS 10.13+ (High Sierra and
//! later). On Apple Silicon (`aarch64`) it is always present; on Intel
//! Macs it depends on the macOS version.
//!
//! This module is a no-op on non-macOS targets — [`has_metal_support`]
//! unconditionally returns `false` there.

use gglib_core::utils::process::cmd;

/// Check if the system has Metal support.
///
/// Returns `true` on Apple Silicon or on Intel Macs running macOS 10.13+.
/// Always returns `false` on non-macOS platforms.
pub fn has_metal_support() -> bool {
    #[cfg(target_os = "macos")]
    {
        // Apple Silicon always has Metal
        if cfg!(target_arch = "aarch64") {
            return true;
        }

        // Intel Macs: Metal requires macOS 10.13+
        if let Ok(output) = cmd("sw_vers").arg("-productVersion").output()
            && let Ok(version) = String::from_utf8(output.stdout)
            && let Some(major) = version.split('.').next()
            && let Ok(major_num) = major.trim().parse::<u32>()
        {
            return major_num >= 10;
        }
    }

    false
}
