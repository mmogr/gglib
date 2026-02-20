//! Generic command existence and version extraction.
//!
//! These functions check if tools exist and extract their version strings.

use std::process::Command;

/// Check if a command exists in the system PATH.
#[cfg(test)]
fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get the version of a command by running it with --version.
pub fn get_command_version(cmd: &str, version_flag: &str) -> Option<String> {
    let output = Command::new(cmd).arg(version_flag).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Try stdout first, fall back to stderr (some tools output to stderr)
    let text = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };

    // Extract version from first line
    text.lines().next().map(|s| s.trim().to_string())
}

/// Get cargo version.
pub fn get_cargo_version() -> Option<String> {
    let output = get_command_version("cargo", "--version")?;
    // "cargo 1.75.0 (1d8b05cdd 2023-11-20)" -> "1.75.0"
    output.split_whitespace().nth(1).map(|s| s.to_string())
}

/// Get rustc version.
pub fn get_rustc_version() -> Option<String> {
    let output = get_command_version("rustc", "--version")?;
    // "rustc 1.75.0 (82e1608df 2023-12-21)" -> "1.75.0"
    output.split_whitespace().nth(1).map(|s| s.to_string())
}

/// Get node version.
pub fn get_node_version() -> Option<String> {
    let output = get_command_version("node", "--version")?;
    // "v20.10.0" -> "20.10.0"
    output.trim_start_matches('v').to_string().into()
}

/// Get npm version.
pub fn get_npm_version() -> Option<String> {
    let output = get_command_version("npm", "--version")?;
    // "10.2.3"
    Some(output.trim().to_string())
}

/// Get git version.
pub fn get_git_version() -> Option<String> {
    let output = get_command_version("git", "--version")?;
    // "git version 2.43.0" -> "2.43.0"
    output.split_whitespace().nth(2).map(|s| s.to_string())
}

/// Get cmake version.
pub fn get_cmake_version() -> Option<String> {
    let output = get_command_version("cmake", "--version")?;
    // "cmake version 3.28.1" -> "3.28.1"
    output.split_whitespace().nth(2).map(|s| s.to_string())
}

/// Get make version.
pub fn get_make_version() -> Option<String> {
    let output = get_command_version("make", "--version")?;
    // "GNU Make 4.4.1" or "make: unknown option -- version" on BSD
    output.split_whitespace().nth(2).map(|s| s.to_string())
}

/// Get gcc version.
pub fn get_gcc_version() -> Option<String> {
    let output = get_command_version("gcc", "--version")?;
    // Extract version from first line, usually "gcc (Ubuntu 13.2.0-4ubuntu3) 13.2.0"
    // or on macOS: "Apple clang version 15.0.0 (clang-1500.1.0.2.5)"
    let line = output.lines().next()?;

    // Try to extract version number patterns
    for word in line.split_whitespace() {
        // Look for patterns like "13.2.0" or "15.0.0"
        if word.chars().next()?.is_ascii_digit() && word.contains('.') {
            return Some(word.to_string());
        }
    }

    // Fallback: try extracting from parentheses for clang
    if line.contains("clang version") {
        return line
            .split("clang version")
            .nth(1)?
            .split_whitespace()
            .next()
            .map(|s| s.to_string());
    }

    None
}

/// Get g++ version.
pub fn get_gxx_version() -> Option<String> {
    let output = get_command_version("g++", "--version")?;
    let line = output.lines().next()?;

    for word in line.split_whitespace() {
        if word.chars().next()?.is_ascii_digit() && word.contains('.') {
            return Some(word.to_string());
        }
    }

    if line.contains("clang version") {
        return line
            .split("clang version")
            .nth(1)?
            .split_whitespace()
            .next()
            .map(|s| s.to_string());
    }

    None
}

/// Get pkg-config version.
pub fn get_pkgconfig_version() -> Option<String> {
    let output = get_command_version("pkg-config", "--version")?;
    Some(output.trim().to_string())
}

/// Get python3 version.
/// Tries `python3` first, then `python` (checking it's Python 3).
pub fn get_python3_version() -> Option<String> {
    // Try python3 first
    if let Some(output) = get_command_version("python3", "--version")
        // "Python 3.12.1" -> "3.12.1"
        && let Some(version) = output.split_whitespace().nth(1)
        && version.starts_with('3')
    {
        return Some(version.to_string());
    }
    // Fallback to python (might be python3 on some systems)
    if let Some(output) = get_command_version("python", "--version")
        && let Some(version) = output.split_whitespace().nth(1)
        && version.starts_with('3')
    {
        return Some(version.to_string());
    }
    None
}

/// Get patchelf version (Linux only).
#[cfg(target_os = "linux")]
pub fn get_patchelf_version() -> Option<String> {
    let output = get_command_version("patchelf", "--version")?;
    // "patchelf 0.18.0" -> "0.18.0"
    output.split_whitespace().nth(1).map(|s| s.to_string())
}

/// Parse a version string into (major, minor) tuple.
#[cfg(test)]
fn parse_version_tuple(version: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 2 {
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        Some((major, minor))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_exists_with_common_command() {
        // 'which' should exist on any Unix system
        assert!(command_exists("ls") || command_exists("dir"));
    }

    #[test]
    fn test_command_exists_with_nonexistent() {
        assert!(!command_exists("definitely_not_a_real_command_12345"));
    }

    #[test]
    fn test_parse_version_tuple_valid() {
        assert_eq!(parse_version_tuple("1.75.0"), Some((1, 75)));
        assert_eq!(parse_version_tuple("12.0"), Some((12, 0)));
    }

    #[test]
    fn test_parse_version_tuple_invalid() {
        assert_eq!(parse_version_tuple("invalid"), None);
        assert_eq!(parse_version_tuple("1"), None);
    }

    #[test]
    fn test_get_cargo_version_format() {
        // If cargo is installed, version should be in X.Y.Z format
        if let Some(version) = get_cargo_version() {
            assert!(
                version.contains('.'),
                "Version should contain dots: {version}"
            );
        }
    }
}
