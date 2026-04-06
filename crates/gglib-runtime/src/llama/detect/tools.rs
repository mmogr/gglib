//! Shared utilities for hardware acceleration detection.
//!
//! This module centralises command-execution helpers and version-parsing
//! routines used across the acceleration-detection submodules (`cuda`,
//! `metal`, `vulkan`). Every submodule should import from here rather
//! than duplicating `std::process::Command` boilerplate or version logic.
//!
//! # Design rationale
//!
//! Hardware detection inevitably shells out to system tools (`nvcc`,
//! `vulkaninfo`, `cmake`, etc.). Repeating the spawn → check status →
//! read stdout pattern in every call-site is error-prone and violates
//! DRY. The helpers here provide a small, typed API on top of
//! [`gglib_core::utils::process::cmd`].

use anyhow::Result;
use gglib_core::utils::process::cmd;

// ============================================================================
// Command-execution helpers
// ============================================================================

/// Run a command and return `true` if it exits successfully.
///
/// Returns `false` if the command cannot be found or exits with a non-zero
/// status.
pub fn command_succeeds(program: &str, args: &[&str]) -> bool {
    cmd(program)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run a command and return its stdout as a trimmed `String` on success.
///
/// Returns `None` if the command cannot be found, exits with a non-zero
/// status, or produces non-UTF-8 output.
pub fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = cmd(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(stdout.trim().to_string())
}

/// Check whether a program is available in `$PATH`.
///
/// A thin wrapper around [`command_succeeds`] using `--version` as a
/// probe argument.
pub fn command_exists(program: &str) -> bool {
    command_succeeds(program, &["--version"])
}

// ============================================================================
// Version parsing
// ============================================================================

/// Parse a version string into a `(major, minor)` tuple.
///
/// Extracts only the leading numeric portion of each component, so
/// `"12.0-rc1"` successfully parses as `(12, 0)`.
pub fn parse_version_tuple(version_str: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() >= 2 {
        let parse_numeric = |part: &str| -> Option<u32> {
            let numeric_str: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            numeric_str.parse::<u32>().ok()
        };
        let major = parse_numeric(parts[0])?;
        let minor = parse_numeric(parts[1])?;
        Some((major, minor))
    } else {
        None
    }
}

// ============================================================================
// Build-tool detection
// ============================================================================

/// Check if git is installed, returning its version string on success.
pub fn has_git() -> Result<Option<String>> {
    match command_stdout("git", &["--version"]) {
        Some(v) => {
            let version = v.strip_prefix("git version ").unwrap_or(&v).to_string();
            Ok(Some(version))
        }
        None => Ok(None),
    }
}

/// Check if cmake is installed, returning its version string on success.
pub fn has_cmake() -> Result<Option<String>> {
    match command_stdout("cmake", &["--version"]) {
        Some(v) => {
            let version = v
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(2))
                .unwrap_or("unknown")
                .to_string();
            Ok(Some(version))
        }
        None => Ok(None),
    }
}

/// Check if a C++ compiler is installed, returning a description on success.
///
/// Tries compilers in platform-preferred order:
/// - **Windows**: `cl`, `g++`, `clang++`
/// - **macOS**: `clang++`, `g++`
/// - **Linux**: `g++`, `clang++`
pub fn has_cpp_compiler() -> Result<Option<String>> {
    let compilers = if cfg!(target_os = "windows") {
        vec!["cl", "g++", "clang++"]
    } else if cfg!(target_os = "macos") {
        vec!["clang++", "g++"]
    } else {
        vec!["g++", "clang++"]
    };

    for compiler in compilers {
        if let Some(stdout) = command_stdout(compiler, &["--version"]) {
            let first_line = stdout.lines().next().unwrap_or("unknown");
            return Ok(Some(format!("{} ({})", compiler, first_line.trim())));
        }
    }

    Ok(None)
}

/// Get the number of CPU cores available for parallel compilation.
pub fn get_num_cores() -> usize {
    num_cpus::get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_tuple_normal() {
        assert_eq!(parse_version_tuple("12.3"), Some((12, 3)));
        assert_eq!(parse_version_tuple("1.0.4"), Some((1, 0)));
    }

    #[test]
    fn test_parse_version_tuple_with_suffix() {
        assert_eq!(parse_version_tuple("12.0-rc1"), Some((12, 0)));
    }

    #[test]
    fn test_parse_version_tuple_invalid() {
        assert_eq!(parse_version_tuple("12"), None);
        assert_eq!(parse_version_tuple(""), None);
    }

    #[test]
    fn test_get_num_cores() {
        let cores = get_num_cores();
        assert!(cores > 0);
        assert!(cores <= 1024);
    }
}
