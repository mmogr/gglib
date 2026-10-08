//! The tools a llama.cpp source build runs, and how to install them.
//!
//! One answer to "can this machine build llama.cpp": `gglib config llama
//! install`, the install a first `gglib serve` or `gglib up` offers and the
//! update preflight all ask [`build_tools`], and all name the same remedy,
//! [`build_tool_install_lines`]. Nothing here prints; the caller owns the
//! terminal, or has none.

use super::detect::tools::version_line;

/// A tool a source build runs, and what it said when asked for its version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildTool {
    /// The name a report gives it: `git`, `cmake` or `C++ compiler`.
    pub name: &'static str,
    /// What it reported, or `None` when it is not on `PATH`.
    pub found: Option<String>,
}

/// Probe git, cmake and a C++ compiler, in the order a build needs them.
#[must_use]
pub fn build_tools() -> Vec<BuildTool> {
    vec![
        BuildTool {
            name: "git",
            found: version_line("git").map(|line| {
                line.strip_prefix("git version ")
                    .unwrap_or(&line)
                    .to_string()
            }),
        },
        BuildTool {
            name: "cmake",
            found: version_line("cmake").map(|line| {
                line.split_whitespace()
                    .nth(2)
                    .unwrap_or("unknown")
                    .to_string()
            }),
        },
        BuildTool {
            name: "C++ compiler",
            found: cpp_compiler(),
        },
    ]
}

/// The names of the tools in `tools` that were not found.
#[must_use]
pub fn missing_build_tools(tools: &[BuildTool]) -> Vec<&'static str> {
    tools
        .iter()
        .filter(|tool| tool.found.is_none())
        .map(|tool| tool.name)
        .collect()
}

/// The first C++ compiler on `PATH`, as `name (its banner)`.
///
/// Tried in the order the platform prefers:
/// - **Windows**: `cl`, `g++`, `clang++`
/// - **macOS**: `clang++`, `g++`
/// - **Linux**: `g++`, `clang++`
fn cpp_compiler() -> Option<String> {
    let compilers: &[&str] = if cfg!(target_os = "windows") {
        &["cl", "g++", "clang++"]
    } else if cfg!(target_os = "macos") {
        &["clang++", "g++"]
    } else {
        &["g++", "clang++"]
    };

    compilers
        .iter()
        .find_map(|compiler| version_line(compiler).map(|banner| format!("{compiler} ({banner})")))
}

/// How to install the build tools on this machine: a heading naming the
/// platform, then what to run, indented under it.
#[must_use]
pub fn build_tool_install_lines() -> Vec<String> {
    #[cfg(target_os = "macos")]
    let lines = [
        "macOS:",
        "  xcode-select --install",
        "  brew install cmake git",
    ]
    .map(str::to_string)
    .to_vec();

    #[cfg(target_os = "windows")]
    let lines = [
        "Windows:",
        "  1. Install Visual Studio 2022 with C++ tools",
        "     https://visualstudio.microsoft.com/downloads/",
        "  2. Install CMake from: https://cmake.org/download/",
        "  3. Install Git from: https://git-scm.com/download/win",
    ]
    .map(str::to_string)
    .to_vec();

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let lines = linux_install_lines(crate::system::detect_linux_distro());

    lines
}

/// The Linux lines of [`build_tool_install_lines`].
///
/// The build needs a compiler, cmake and git; the shared package table knows
/// what each is called on `distro`. Naming them through it rather than inline
/// keeps this in step with `gglib config check-deps`, which reads the same
/// table.
#[cfg(any(not(any(target_os = "macos", target_os = "windows")), test))]
fn linux_install_lines(distro: gglib_core::utils::system::LinuxDistro) -> Vec<String> {
    let packages: Vec<&str> = ["gcc", "cmake", "git"]
        .iter()
        .filter_map(|dependency| {
            gglib_core::utils::system::packages_for(dependency)
                .and_then(|names| names.for_distro(distro))
        })
        .collect();

    let install = match (distro.installer(), packages.is_empty()) {
        (Some(installer), false) => format!("  sudo {installer} {}", packages.join(" ")),
        _ => "  Install: a C/C++ toolchain, cmake, git".to_string(),
    };

    vec![format!("{}:", distro.label()), install]
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::utils::system::LinuxDistro;

    fn tool(name: &'static str, found: Option<&str>) -> BuildTool {
        BuildTool {
            name,
            found: found.map(str::to_string),
        }
    }

    #[test]
    fn the_missing_tools_are_the_ones_that_reported_nothing() {
        let all = [
            tool("git", Some("2.43.0")),
            tool("cmake", Some("3.28.1")),
            tool("C++ compiler", Some("g++ (g++ 13.2.0)")),
        ];
        assert!(missing_build_tools(&all).is_empty());

        let some = [
            tool("git", None),
            tool("cmake", Some("3.28.1")),
            tool("C++ compiler", None),
        ];
        assert_eq!(missing_build_tools(&some), ["git", "C++ compiler"]);
    }

    /// The three tools are always reported, found or not, in the order a
    /// report prints them.
    #[test]
    fn the_probe_reports_git_cmake_and_a_compiler_in_that_order() {
        let names: Vec<&str> = build_tools().iter().map(|tool| tool.name).collect();
        assert_eq!(names, ["git", "cmake", "C++ compiler"]);
    }

    /// Each family is told its own package manager and its own package
    /// names, and an unidentified one is told what to look for.
    #[test]
    fn a_linux_family_is_told_its_own_install_command() {
        for (distro, lines) in [
            (
                LinuxDistro::Debian,
                [
                    "Debian/Ubuntu:",
                    "  sudo apt install build-essential cmake git",
                ],
            ),
            (
                LinuxDistro::Fedora,
                [
                    "Fedora/RHEL:",
                    "  sudo dnf install gcc gcc-c++ make cmake git",
                ],
            ),
            (
                LinuxDistro::Arch,
                ["Arch Linux:", "  sudo pacman -S base-devel cmake git"],
            ),
            (
                LinuxDistro::Unknown,
                ["Linux:", "  Install: a C/C++ toolchain, cmake, git"],
            ),
        ] {
            assert_eq!(linux_install_lines(distro), lines, "{distro:?}");
        }
    }

    /// A heading, then at least one indented line under it, on any platform.
    #[test]
    fn the_install_lines_are_a_heading_and_what_to_run() {
        let lines = build_tool_install_lines();
        assert!(lines[0].ends_with(':'), "{lines:?}");
        assert!(lines.len() >= 2, "{lines:?}");
        assert!(lines[1..].iter().all(|line| line.starts_with("  ")));
    }
}
