//! Library and dependency-specific checks.
//!
//! These functions check for specific system libraries using pkg-config.

use gglib_core::utils::process::cmd;

/// Check if libssl-dev is installed by checking for OpenSSL with pkg-config.
pub(super) fn check_libssl() -> Option<String> {
    check_pkg_config_lib("openssl")
}

/// Check if libsqlite3-dev is installed
#[cfg(target_os = "linux")]
pub(super) fn check_libsqlite3() -> Option<String> {
    check_pkg_config_lib("sqlite3")
}

/// Check if libasound2-dev is installed (ALSA for audio support)
#[cfg(target_os = "linux")]
pub(super) fn check_libasound() -> Option<String> {
    check_pkg_config_lib("alsa")
}

/// Check if libcurl-dev is installed
#[cfg(target_os = "linux")]
pub(super) fn check_libcurl() -> Option<String> {
    check_pkg_config_lib("libcurl")
}

/// Check if libclang-dev is installed (needed by bindgen for FFI bindings).
///
/// libclang doesn't have a pkg-config file, so we check for the shared library
/// directly: where llvm-config says it is, then the standard library paths,
/// then the per-version LLVM directories.
#[cfg(target_os = "linux")]
pub(super) fn check_libclang() -> Option<String> {
    use std::path::Path;

    if let Ok(output) = cmd("llvm-config").arg("--libdir").output()
        && output.status.success()
        && dir_has_libclang(Path::new(String::from_utf8_lossy(&output.stdout).trim()))
    {
        // The LLVM version, for display.
        if let Ok(ver_output) = cmd("llvm-config").arg("--version").output()
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

    if ["/usr/lib/x86_64-linux-gnu", "/usr/lib/aarch64-linux-gnu"]
        .iter()
        .any(|dir| dir_has_libclang(Path::new(dir)))
    {
        return Some("installed".to_string());
    }

    (11..=20)
        .rev()
        .find(|major| dir_has_libclang(Path::new(&format!("/usr/lib/llvm-{major}/lib"))))
        .map(|major| major.to_string())
}

/// Whether `dir` holds a libclang shared library.
#[cfg(any(target_os = "linux", test))]
fn dir_has_libclang(dir: &std::path::Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("libclang") && name.contains(".so")
        })
    })
}

/// Check for a library using pkg-config.
pub(super) fn check_pkg_config_lib(lib_name: &str) -> Option<String> {
    let output = cmd("pkg-config")
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
pub(super) fn check_webkit2gtk() -> Option<String> {
    // Try webkit2gtk-4.1 first (Ubuntu 24.04+)
    if let Some(version) = check_pkg_config_lib("webkit2gtk-4.1") {
        return Some(version);
    }
    // Fall back to webkit2gtk-4.0 (older versions)
    check_pkg_config_lib("webkit2gtk-4.0")
}

/// Check if librsvg is installed.
#[cfg(target_os = "linux")]
pub(super) fn check_librsvg() -> Option<String> {
    check_pkg_config_lib("librsvg-2.0")
}

/// Check if libappindicator-gtk3 is installed.
#[cfg(target_os = "linux")]
pub(super) fn check_libappindicator() -> Option<String> {
    // Try ayatana-appindicator first (newer Ubuntu/Debian)
    if let Some(version) = check_pkg_config_lib("ayatana-appindicator3-0.1") {
        return Some(version);
    }
    // Fall back to older appindicator
    check_pkg_config_lib("appindicator3-0.1")
}

/// Check if gtk-layer-shell is installed.
///
/// Optional, unlike the rest of this module: without it the tray panel opens
/// wherever the compositor puts it instead of beside the system tray, which is
/// a worse panel rather than a broken build.
#[cfg(target_os = "linux")]
pub(super) fn check_gtk_layer_shell() -> Option<String> {
    check_pkg_config_lib("gtk-layer-shell-0")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory that is not there holds none, and neither does one whose
    /// only matches are libclang's static archive or another library's `.so`.
    #[test]
    fn a_directory_holds_libclang_when_a_shared_library_of_that_name_is_in_it() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!dir_has_libclang(&dir.path().join("absent")));
        assert!(!dir_has_libclang(dir.path()));

        std::fs::write(dir.path().join("libclang.a"), "").unwrap();
        std::fs::write(dir.path().join("libLLVM-18.so"), "").unwrap();
        assert!(!dir_has_libclang(dir.path()));

        std::fs::write(dir.path().join("libclang-18.so.1"), "").unwrap();
        assert!(dir_has_libclang(dir.path()));
    }

    #[test]
    fn test_check_pkg_config_lib_nonexistent() {
        // A library that definitely doesn't exist
        assert!(check_pkg_config_lib("nonexistent-library-12345").is_none());
    }
}
