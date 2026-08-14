//! Path resolution and validation for MCP server executables.
//!
//! This module provides utilities to:
//! - Validate executable paths (absolute, exists, executable permission)
//! - Build effective PATH for child processes (includes exe dir, system paths, custom paths)
//! - Validate working directories

use std::env;
use std::ffi::OsString;
use std::path::Path;

/// Platform-specific PATH separator
#[cfg(unix)]
const PATH_SEPARATOR: &str = ":";
#[cfg(windows)]
const PATH_SEPARATOR: &str = ";";

/// Default paths to include on macOS when PATH is limited (bundled apps)
#[cfg(target_os = "macos")]
const MACOS_DEFAULT_PATHS: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";

/// Validate an executable path.
///
/// Returns Ok(()) if:
/// - Path is absolute
/// - File exists
/// - File is executable (Unix) or spawnable
pub fn validate_exe_path(exe_path: &str) -> Result<(), String> {
    let path = Path::new(exe_path);

    // Must be absolute
    if !path.is_absolute() {
        return Err(format!("Executable path must be absolute: {exe_path}"));
    }

    // Must exist
    if !path.exists() {
        return Err(format!("Executable not found: {exe_path}"));
    }

    // Must be a file
    if !path.is_file() {
        return Err(format!("Executable path is not a file: {exe_path}"));
    }

    // Check if executable (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(metadata) => {
                let permissions = metadata.permissions();
                if permissions.mode() & 0o111 == 0 {
                    return Err(format!("File is not executable: {exe_path}"));
                }
            }
            Err(e) => return Err(format!("Failed to check permissions: {e}")),
        }
    }

    Ok(())
}

/// Validate a working directory.
///
/// Returns Ok(()) if the directory exists and is actually a directory.
pub fn validate_working_dir(cwd: &str) -> Result<(), String> {
    let path = Path::new(cwd);

    if !path.exists() {
        return Err(format!("Working directory does not exist: {cwd}"));
    }

    if !path.is_dir() {
        return Err(format!("Working directory path is not a directory: {cwd}"));
    }

    Ok(())
}

/// Build an effective PATH for the child process.
///
/// This includes:
/// 1. Directory containing the executable (so scripts can find their interpreters)
/// 2. Current process PATH
/// 3. Platform-specific default paths (macOS: Homebrew, etc.)
/// 4. Optional user-provided `path_extra`
///
/// Entries are deduplicated.
pub fn build_effective_path(exe_path: &str, path_extra: Option<&str>) -> OsString {
    let mut path_entries = Vec::new();

    // 1. Add directory containing the executable
    if let Some(exe_dir) = Path::new(exe_path).parent() {
        if let Some(dir_str) = exe_dir.to_str() {
            path_entries.push(dir_str.to_string());
        }
    }

    // 2. Add current process PATH
    if let Some(current_path) = env::var_os("PATH") {
        if let Some(current_path_str) = current_path.to_str() {
            for entry in current_path_str.split(PATH_SEPARATOR) {
                if !entry.is_empty() {
                    path_entries.push(entry.to_string());
                }
            }
        }
    }

    // 3. Add platform-specific defaults (especially important for macOS bundled apps)
    #[cfg(target_os = "macos")]
    {
        for entry in MACOS_DEFAULT_PATHS.split(':') {
            if !entry.is_empty() {
                path_entries.push(entry.to_string());
            }
        }
    }

    // 4. Add user-provided path_extra
    if let Some(extra) = path_extra {
        for entry in extra.split(PATH_SEPARATOR) {
            if !entry.is_empty() {
                path_entries.push(entry.to_string());
            }
        }
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    let deduped: Vec<String> = path_entries
        .into_iter()
        .filter(|entry| seen.insert(entry.clone()))
        .collect();

    // Join with platform-specific separator
    OsString::from(deduped.join(PATH_SEPARATOR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_exe_path_rejects_relative() {
        let result = validate_exe_path("node");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("absolute"));
    }

    #[test]
    fn test_validate_exe_path_rejects_nonexistent() {
        let result = validate_exe_path("/nonexistent/path/to/exe");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_build_effective_path_includes_exe_dir() {
        let exe_path = "/opt/homebrew/bin/npx";
        let path = build_effective_path(exe_path, None);
        let path_str = path.to_str().unwrap();
        assert!(path_str.contains("/opt/homebrew/bin"));
    }

    #[test]
    fn test_build_effective_path_deduplicates() {
        let exe_path = "/usr/bin/node";
        // /usr/bin will be added from exe_path and likely from system PATH
        let path = build_effective_path(exe_path, Some("/usr/bin:/custom/path"));
        let path_str = path.to_str().unwrap();

        // Count exact occurrences of /usr/bin as a PATH entry (not substring)
        let entries: Vec<&str> = path_str.split(PATH_SEPARATOR).collect();
        let count = entries.iter().filter(|&&e| e == "/usr/bin").count();
        assert_eq!(count, 1, "PATH should deduplicate /usr/bin");
    }

    #[test]
    fn test_validate_working_dir_rejects_nonexistent() {
        let result = validate_working_dir("/nonexistent/directory");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }
}
