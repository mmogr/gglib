//! Library and dependency-specific checks.
//!
//! These functions check for specific system libraries using pkg-config.

use std::process::Command;

/// Check if libssl-dev is installed by checking for OpenSSL with pkg-config.
pub fn check_libssl() -> Option<String> {
    check_pkg_config_lib("openssl")
}

/// Check for a library using pkg-config.
pub fn check_pkg_config_lib(lib_name: &str) -> Option<String> {
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

/// Check if webkit2gtk is installed (tries 4.1 then falls back to 4.0).
#[cfg(target_os = "linux")]
pub fn check_webkit2gtk() -> Option<String> {
    // Try webkit2gtk-4.1 first (Ubuntu 24.04+)
    if let Some(version) = check_pkg_config_lib("webkit2gtk-4.1") {
        return Some(version);
    }
    // Fall back to webkit2gtk-4.0 (older versions)
    check_pkg_config_lib("webkit2gtk-4.0")
}

/// Check if librsvg is installed.
#[cfg(target_os = "linux")]
pub fn check_librsvg() -> Option<String> {
    check_pkg_config_lib("librsvg-2.0")
}

/// Check if libappindicator-gtk3 is installed.
#[cfg(target_os = "linux")]
pub fn check_libappindicator() -> Option<String> {
    // Try ayatana-appindicator first (newer Ubuntu/Debian)
    if let Some(version) = check_pkg_config_lib("ayatana-appindicator3-0.1") {
        return Some(version);
    }
    // Fall back to older appindicator
    check_pkg_config_lib("appindicator3-0.1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_pkg_config_lib_nonexistent() {
        // A library that definitely doesn't exist
        assert!(check_pkg_config_lib("nonexistent-library-12345").is_none());
    }
}
