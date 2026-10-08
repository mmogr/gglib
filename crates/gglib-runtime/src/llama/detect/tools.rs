//! How this crate asks a tool on `PATH` what it is.
//!
//! This module centralises command-execution helpers and version-parsing
//! routines used across the acceleration-detection submodules (`cuda`,
//! `metal`, `vulkan`), by the llama.cpp build-tool check and by the system
//! dependency list. Every one of them imports from here rather than
//! duplicating `std::process::Command` boilerplate or version logic, so two
//! reports about the same tool cannot disagree about whether it is there.
//!
//! # Design rationale
//!
//! Hardware detection inevitably shells out to system tools (`nvcc`,
//! `vulkaninfo`, `cmake`, etc.). Repeating the spawn → check status →
//! read stdout pattern in every call-site is error-prone and violates
//! DRY. The helpers here provide a small, typed API on top of
//! [`gglib_core::utils::process::cmd`].

use gglib_core::utils::process::cmd;

// ============================================================================
// Command-execution helpers
// ============================================================================

/// Run a command and return `true` if it exits successfully.
///
/// Returns `false` if the command cannot be found or exits with a non-zero
/// status.
pub(crate) fn command_succeeds(program: &str, args: &[&str]) -> bool {
    cmd(program)
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Run a command and return its stdout as a trimmed `String` on success.
///
/// Returns `None` if the command cannot be found, exits with a non-zero
/// status, or produces non-UTF-8 output.
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub(crate) fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
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
pub(crate) fn command_exists(program: &str) -> bool {
    command_succeeds(program, &["--version"])
}

// ============================================================================
// Version parsing
// ============================================================================

/// Parse a version string into a `(major, minor)` tuple.
///
/// Extracts only the leading numeric portion of each component, so
/// `"12.0-rc1"` successfully parses as `(12, 0)`.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn parse_version_tuple(version_str: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = version_str.split('.').collect();
    if parts.len() >= 2 {
        let parse_numeric = |part: &str| -> Option<u32> {
            let numeric_str: String = part.chars().take_while(char::is_ascii_digit).collect();
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
// Version banners
// ============================================================================

/// The first line `program --version` prints, when the program runs and
/// exits successfully.
///
/// Read from stdout, or from stderr when stdout is empty: some tools write
/// their banner there.
pub(crate) fn version_line(program: &str) -> Option<String> {
    first_line(&cmd(program).arg("--version").output().ok()?)
}

fn first_line(output: &std::process::Output) -> Option<String> {
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stdout.trim().is_empty() {
        stderr
    } else {
        stdout
    };

    text.lines().next().map(|line| line.trim().to_string())
}

/// One whitespace-separated word of [`version_line`], counted from zero:
/// word 1 of `cargo 1.75.0 (1d8b05cdd 2023-11-20)`, word 2 of
/// `git version 2.43.0`.
pub(crate) fn version_word(program: &str, index: usize) -> Option<String> {
    word(&version_line(program)?, index)
}

fn word(line: &str, index: usize) -> Option<String> {
    line.split_whitespace().nth(index).map(str::to_string)
}

/// The version `gcc` or `g++` reports, whichever compiler answers to the name.
pub(crate) fn compiler_version(program: &str) -> Option<String> {
    compiler_version_in(&version_line(program)?)
}

/// The version in a compiler's banner: its first word that starts with a
/// digit and holds a dot, or else what follows `clang version`.
fn compiler_version_in(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()) && word.contains('.'))
        .or_else(|| {
            line.split_once("clang version")
                .and_then(|(_, rest)| rest.split_whitespace().next())
        })
        .map(str::to_string)
}

/// The version of Python 3, asked of `python3` and then of `python`, which
/// is Python 3 on some systems.
pub(crate) fn python3_version() -> Option<String> {
    ["python3", "python"]
        .into_iter()
        .filter_map(|program| version_word(program, 1))
        .find(|version| version.starts_with('3'))
}

/// Get the number of CPU cores available for parallel compilation.
pub(crate) fn get_num_cores() -> usize {
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

    /// The banners are the ones each tool prints. `gcc` on Ubuntu shows that
    /// the first word with a digit and a dot wins, bracket and all: the
    /// dependency check has always printed it that way.
    #[test]
    fn a_compiler_version_is_the_first_dotted_number_in_its_banner() {
        for (banner, version) in [
            ("gcc (GCC) 14.2.1 20240910", Some("14.2.1")),
            ("g++ (Debian 12.2.0-14) 12.2.0", Some("12.2.0-14)")),
            (
                "gcc (Ubuntu 13.2.0-4ubuntu3) 13.2.0",
                Some("13.2.0-4ubuntu3)"),
            ),
            (
                "Apple clang version 15.0.0 (clang-1500.1.0.2.5)",
                Some("15.0.0"),
            ),
            ("gcc.exe (MinGW.org GCC-6.3.0-1) 6.3.0", Some("6.3.0")),
            ("clang version trunk", Some("trunk")),
            ("cc", None),
            ("", None),
        ] {
            assert_eq!(
                compiler_version_in(banner).as_deref(),
                version,
                "{banner:?}"
            );
        }
    }

    #[test]
    fn a_version_word_is_counted_from_zero_and_may_be_absent() {
        let banner = "cmake version 3.28.1";
        assert_eq!(word(banner, 2).as_deref(), Some("3.28.1"));
        assert_eq!(word(banner, 0).as_deref(), Some("cmake"));
        assert_eq!(word(banner, 3), None);
        assert_eq!(word("  GNU   Make  4.4.1 ", 2).as_deref(), Some("4.4.1"));
    }

    /// A run that fails has no banner, and one that answers on stderr alone
    /// is still heard.
    #[cfg(unix)]
    #[test]
    fn a_banner_is_the_first_line_a_successful_run_prints() {
        use std::os::unix::process::ExitStatusExt;
        use std::process::{ExitStatus, Output};

        let run = |code: i32, stdout: &str, stderr: &str| Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        };

        assert_eq!(
            first_line(&run(0, "  tool 1.2.3  \nsecond line\n", "a warning\n")).as_deref(),
            Some("tool 1.2.3")
        );
        assert_eq!(
            first_line(&run(0, " \n", "tool 4.5.6\n")).as_deref(),
            Some("tool 4.5.6")
        );
        assert_eq!(first_line(&run(3, "tool 7.8.9\n", "")), None);
        assert_eq!(first_line(&run(0, "", "")), None);
        assert_eq!(version_line("gglib-no-such-program-on-any-path"), None);
    }

    #[test]
    fn test_get_num_cores() {
        let cores = get_num_cores();
        assert!(cores > 0);
        assert!(cores <= 1024);
    }
}
