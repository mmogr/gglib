//! Path resolution and validation for MCP server executables.
//!
//! This module provides utilities to:
//! - Validate executable paths (absolute, exists, executable permission)
//! - Build effective PATH for child processes (includes exe dir, system paths, custom paths)
//! - Validate working directories

use crate::resolver::{AttemptOutcome, FsProvider, PATH_SEPARATOR, SystemFs};
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::Path;

/// Validate an executable path.
///
/// Returns Ok(()) if:
/// - Path is absolute
/// - File exists
/// - File is executable (Unix) or spawnable
///
/// The check is the resolver's, so a path it resolved is one this accepts.
pub(crate) fn validate_exe_path(exe_path: &str) -> Result<(), String> {
    let path = Path::new(exe_path);

    // Must be absolute
    if !path.is_absolute() {
        return Err(format!("Executable path must be absolute: {exe_path}"));
    }

    match SystemFs.check_executable(path) {
        AttemptOutcome::Ok => Ok(()),
        AttemptOutcome::NotFound => Err(format!("Executable not found: {exe_path}")),
        AttemptOutcome::NotAFile => Err(format!("Executable path is not a file: {exe_path}")),
        AttemptOutcome::NotExecutable => Err(format!("File is not executable: {exe_path}")),
        AttemptOutcome::PermissionDenied => {
            Err("Failed to check permissions: permission denied".to_string())
        }
        AttemptOutcome::IoError(e) => Err(format!("Failed to check permissions: {e}")),
    }
}

/// Validate a working directory.
///
/// Returns Ok(()) if the directory exists and is actually a directory.
pub(crate) fn validate_working_dir(cwd: &str) -> Result<(), String> {
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
pub(crate) fn build_effective_path(exe_path: &str, path_extra: Option<&str>) -> OsString {
    effective_path(exe_path, env::var_os("PATH").as_deref(), path_extra)
}

/// [`build_effective_path`], given the current process PATH.
fn effective_path(
    exe_path: &str,
    current_path: Option<&OsStr>,
    path_extra: Option<&str>,
) -> OsString {
    let mut path_entries = Vec::new();

    // 1. Add directory containing the executable
    if let Some(exe_dir) = Path::new(exe_path).parent() {
        if let Some(dir_str) = exe_dir.to_str() {
            path_entries.push(dir_str.to_string());
        }
    }

    // 2. Add current process PATH
    if let Some(current_path) = current_path {
        if let Some(current_path_str) = current_path.to_str() {
            for entry in current_path_str.split(PATH_SEPARATOR) {
                if !entry.is_empty() {
                    path_entries.push(entry.to_string());
                }
            }
        }
    }

    // 3. Add platform-specific defaults (especially important for macOS bundled apps):
    //    the resolver's default directories, then the system ones. A child may
    //    need those; they are not in the resolver's default list.
    #[cfg(target_os = "macos")]
    {
        let system_dirs = ["/usr/sbin", "/sbin"];
        for entry in crate::resolver::DEFAULT_DIRS.iter().chain(&system_dirs) {
            path_entries.push((*entry).to_string());
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

    #[cfg(unix)]
    #[test]
    fn an_executable_path_is_refused_with_what_is_wrong_with_it() {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_str().unwrap();
        let file = dir.path().join("tool");
        let file_path = file.to_str().unwrap();

        assert_eq!(
            validate_exe_path("tool"),
            Err("Executable path must be absolute: tool".to_string())
        );
        assert_eq!(
            validate_exe_path(file_path),
            Err(format!("Executable not found: {file_path}"))
        );
        assert_eq!(
            validate_exe_path(dir_path),
            Err(format!("Executable path is not a file: {dir_path}"))
        );
        std::fs::write(&file, "").unwrap();
        std::fs::set_permissions(&file, Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            validate_exe_path(file_path),
            Err(format!("File is not executable: {file_path}"))
        );
        std::fs::set_permissions(&file, Permissions::from_mode(0o755)).unwrap();
        assert_eq!(validate_exe_path(file_path), Ok(()));
    }

    /// The defaults are a macOS matter: an app started from the Finder has a
    /// minimal `PATH`. They are the resolver's default directories and,
    /// after them, the system ones a child may need.
    #[test]
    fn a_childs_path_is_the_executables_directory_then_path_then_the_defaults_then_the_extra() {
        let list = |entries: &[&str]| entries.join(PATH_SEPARATOR);
        let current = OsString::from(list(&["/gglib-test/p", "/usr/bin", "", "/gglib-test/bin"]));
        let extra = list(&["/gglib-test/x", "/usr/bin", "/gglib-test/y"]);

        let built = effective_path("/gglib-test/bin/tool", Some(&current), Some(&extra));

        let mut expected = vec!["/gglib-test/bin", "/gglib-test/p", "/usr/bin"];
        if cfg!(target_os = "macos") {
            let defaults = ["/opt/homebrew/bin", "/usr/local/bin", "/bin"];
            expected.extend(defaults);
            expected.extend(["/usr/sbin", "/sbin"]);
        }
        expected.extend(["/gglib-test/x", "/gglib-test/y"]);
        let built: Vec<&str> = built.to_str().unwrap().split(PATH_SEPARATOR).collect();
        assert_eq!(built, expected);
    }

    #[test]
    fn a_childs_path_is_built_on_the_path_of_this_process() {
        let exe_path = "/gglib-test/bin/tool";
        assert_eq!(
            build_effective_path(exe_path, None),
            effective_path(exe_path, env::var_os("PATH").as_deref(), None)
        );
    }
}
