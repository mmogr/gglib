//! Library and dependency-specific checks.
//!
//! These functions check for specific system libraries using pkg-config.

use std::process::Command;

/// Check if libssl-dev is installed by checking for OpenSSL with pkg-config.
pub fn check_libssl() -> Option<String> {
    check_pkg_config_lib("openssl")
}

/// Check if libsqlite3-dev is installed
#[cfg(target_os = "linux")]
pub fn check_libsqlite3() -> Option<String> {
    check_pkg_config_lib("sqlite3")
}

/// Check if libasound2-dev is installed (ALSA for audio support)
#[cfg(target_os = "linux")]
pub fn check_libasound() -> Option<String> {
    check_pkg_config_lib("alsa")
}

/// Check if libcurl-dev is installed
#[cfg(target_os = "linux")]
pub fn check_libcurl() -> Option<String> {
    check_pkg_config_lib("libcurl")
}

/// Check if libclang-dev is installed (needed by bindgen for FFI bindings).
///
/// libclang doesn't have a pkg-config file, so we check for the shared library
/// directly in standard paths or via llvm-config.
#[cfg(target_os = "linux")]
pub fn check_libclang() -> Option<String> {
    // Method 1: Try llvm-config
    if let Ok(output) = Command::new("llvm-config").arg("--libdir").output()
        && output.status.success()
    {
        let libdir = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let libdir_path = std::path::Path::new(&libdir);
        if libdir_path.exists()
            && let Ok(entries) = std::fs::read_dir(libdir_path)
        {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("libclang") && name_str.contains(".so") {
                    // Get LLVM version for display
                    if let Ok(ver_output) =
                        Command::new("llvm-config").arg("--version").output()
                        && ver_output.status.success()
                    {
                        return Some(
                            String::from_utf8_lossy(&ver_output.stdout)
                                .trim()
                                .to_string(),
                        );
                    }
                    return Some("installed".to_string());
                }
            }
        }
    }

    // Method 2: Check standard library paths
    let search_patterns = ["/usr/lib/x86_64-linux-gnu", "/usr/lib/aarch64-linux-gnu"];

    for dir in &search_patterns {
        let path = std::path::Path::new(dir);
        if path.exists()
            && let Ok(entries) = std::fs::read_dir(path)
        {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("libclang") && name_str.contains(".so") {
                    return Some("installed".to_string());
                }
            }
        }
    }

    // Method 3: Check llvm-specific paths
    for major in (11..=20).rev() {
        let llvm_lib = format!("/usr/lib/llvm-{}/lib", major);
        let path = std::path::Path::new(&llvm_lib);
        if path.exists()
            && let Ok(entries) = std::fs::read_dir(path)
        {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("libclang") && name_str.contains(".so") {
                    return Some(major.to_string());
                }
            }
        }
    }

    None
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
