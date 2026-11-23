//! Path resolution and validation for MCP server executables.
//!
//! This module provides utilities to:
//! - Validate a server's command as one program
//! - Validate executable paths (absolute, exists, executable permission)
//! - Build effective PATH for child processes (includes exe dir, system paths, custom paths)
//! - Validate working directories

use crate::resolver::{AttemptOutcome, FsProvider, Platform, SystemFs};
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::Path;

/// Validate a server's command: one program, its flags and arguments in
/// `args`.
///
/// A command with whitespace in it is taken for a program and its flags
/// typed together, and refused, unless it is the absolute path of a file
/// that exists, as `C:\Program Files\nodejs\npx.cmd` is. An absolute path
/// with a space that is no such file is refused as that. Whether the file is
/// a program is left to the start, which says what is wrong with it.
pub(crate) fn validate_command(command: &str) -> Result<(), String> {
    if !command.contains(char::is_whitespace) {
        return Ok(());
    }
    let path = Path::new(command);
    if !path.is_absolute() {
        return Err(
            "Command must be an executable name/path only (e.g., 'npx'). \
                    Put flags and arguments in the 'args' field."
                .to_string(),
        );
    }
    if path.is_file() {
        return Ok(());
    }
    Err(format!(
        "Command '{command}' has a space in it and is not a file that exists. \
         A command with a space must be the full path to a program; put flags \
         and arguments in the 'args' field."
    ))
}

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

    match SystemFs.check_executable(path, &Platform::host()) {
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
    effective_path(
        exe_path,
        env::var_os("PATH").as_deref(),
        path_extra,
        &Platform::host(),
    )
}

/// [`build_effective_path`], given the current process PATH and the rules
/// its lists are written by.
fn effective_path(
    exe_path: &str,
    current_path: Option<&OsStr>,
    path_extra: Option<&str>,
    platform: &Platform,
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
            path_entries.extend(platform.split(current_path_str).map(String::from));
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
        path_entries.extend(platform.split(extra).map(String::from));
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    let deduped: Vec<String> = path_entries
        .into_iter()
        .filter(|entry| seen.insert(entry.clone()))
        .collect();

    // Join with platform-specific separator
    OsString::from(platform.join(&deduped))
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
        // Absolute on every OS, and missing: a new folder holds nothing.
        let dir = tempfile::tempdir().unwrap();
        let result = validate_exe_path(dir.path().join("exe").to_str().unwrap());
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
        let entries: Vec<&str> = Platform::host().split(path_str).collect();
        let count = entries.iter().filter(|&&e| e == "/usr/bin").count();
        assert_eq!(count, 1, "PATH should deduplicate /usr/bin");
    }

    #[test]
    fn test_validate_working_dir_rejects_nonexistent() {
        let result = validate_working_dir("/nonexistent/directory");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn a_path_that_is_no_file_is_refused_with_what_is_wrong_with_it() {
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
    }

    /// A command with a space in it is a program with its flags typed in,
    /// unless it is the absolute path of a file that exists (#2188).
    #[test]
    fn a_command_has_a_space_only_as_the_path_of_a_file_that_exists() {
        let dir = tempfile::Builder::new()
            .prefix("gglib spaced ")
            .tempdir()
            .unwrap();
        let file = dir.path().join("a tool");
        std::fs::write(&file, "").unwrap();
        let file = file.to_str().unwrap();
        let unspaced = dir.path().join("tool");
        std::fs::write(&unspaced, "").unwrap();
        let unspaced = unspaced.to_str().unwrap();
        let folder = dir.path().to_str().unwrap();
        let missing = dir.path().join("missing tool");
        let missing = missing.to_str().unwrap();
        let with_a_flag = format!("{file} --flag");
        let flags = Err(
            "Command must be an executable name/path only (e.g., 'npx'). \
                         Put flags and arguments in the 'args' field."
                .to_string(),
        );
        let no_file = |command: &str| {
            Err(format!(
                "Command '{command}' has a space in it and is not a file that exists. \
                 A command with a space must be the full path to a program; put flags \
                 and arguments in the 'args' field."
            ))
        };

        assert_eq!(validate_command("npx"), Ok(()));
        assert_eq!(validate_command(file), Ok(()));
        assert_eq!(validate_command(unspaced), Ok(()));
        assert_eq!(validate_command("npx -y pkg"), flags);
        assert_eq!(validate_command("a tool"), flags, "relative");
        assert_eq!(validate_command(missing), no_file(missing));
        assert_eq!(validate_command(folder), no_file(folder));
        assert_eq!(validate_command(&with_a_flag), no_file(&with_a_flag));
    }

    // Unix only: a file is a program by its permission bits, which only Unix
    // has. Windows' half is `on_windows_a_file_is_a_program_by_its_extension`.
    #[cfg(unix)]
    #[test]
    fn on_unix_a_file_is_a_program_by_its_permission_bits() {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("tool");
        let file_path = file.to_str().unwrap();

        std::fs::write(&file, "").unwrap();
        std::fs::set_permissions(&file, Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            validate_exe_path(file_path),
            Err(format!("File is not executable: {file_path}"))
        );
        std::fs::set_permissions(&file, Permissions::from_mode(0o755)).unwrap();
        assert_eq!(validate_exe_path(file_path), Ok(()));
    }

    /// Under this machine's rules a file with no program extension is
    /// refused, and `tool.exe` is accepted.
    #[cfg(windows)]
    #[test]
    fn on_windows_a_file_is_a_program_by_its_extension() {
        let dir = tempfile::tempdir().unwrap();
        let bare = dir.path().join("tool");
        let program = dir.path().join("tool.exe");
        std::fs::write(&bare, "").unwrap();
        std::fs::write(&program, "").unwrap();
        let bare = bare.to_str().unwrap();

        assert_eq!(
            validate_exe_path(bare),
            Err(format!("File is not executable: {bare}"))
        );
        assert_eq!(validate_exe_path(program.to_str().unwrap()), Ok(()));
    }

    /// The defaults are a macOS matter: an app started from the Finder has a
    /// minimal `PATH`. They are the resolver's default directories and,
    /// after them, the system ones a child may need.
    #[test]
    fn a_childs_path_is_the_executables_directory_then_path_then_the_defaults_then_the_extra() {
        let host = Platform::host();
        let list = |entries: &[&str]| host.join(entries);
        let current = OsString::from(list(&["/gglib-test/p", "/usr/bin", "", "/gglib-test/bin"]));
        let extra = list(&["/gglib-test/x", "/usr/bin", "/gglib-test/y"]);

        let built = effective_path("/gglib-test/bin/tool", Some(&current), Some(&extra), &host);

        let mut expected = vec!["/gglib-test/bin", "/gglib-test/p", "/usr/bin"];
        if cfg!(target_os = "macos") {
            let defaults = ["/opt/homebrew/bin", "/usr/local/bin", "/bin"];
            expected.extend(defaults);
            expected.extend(["/usr/sbin", "/sbin"]);
        }
        expected.extend(["/gglib-test/x", "/gglib-test/y"]);
        let built: Vec<&str> = host.split(built.to_str().unwrap()).collect();
        assert_eq!(built, expected);
    }

    #[test]
    fn a_childs_path_is_built_on_the_path_of_this_process() {
        let exe_path = "/gglib-test/bin/tool";
        assert_eq!(
            build_effective_path(exe_path, None),
            effective_path(
                exe_path,
                env::var_os("PATH").as_deref(),
                None,
                &Platform::host()
            )
        );
    }
}
