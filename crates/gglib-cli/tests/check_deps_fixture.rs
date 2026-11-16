//! `gglib config check-deps` on a fixture machine: a `PATH` of stand-ins that
//! answer `--version` with banners the real tools print.
//!
//! The binary a test runs is a build from this checkout, so the report is a
//! checkout's: the build tools are marked as needed. An installed copy's
//! report is tested as a pure function in `gglib_core::utils::system`, on
//! every OS. The rows asserted are the ones macOS and Linux both list; a
//! platform's own rows (the libraries Linux links, the GPU) depend on the
//! machine and are not compared.

#![cfg(unix)]

#[path = "support/stub_tools.rs"]
mod stub_tools;

use gglib_core::utils::process::cmd;
use std::process::Stdio;

use stub_tools::{Machine, one_at_a_time};

/// The same command with its colour taken out.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("\x1b[") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        rest = after.find('m').map_or("", |end| &after[end + 1..]);
    }
    out.push_str(rest);
    out
}

#[test]
fn check_deps_prints_each_tool_under_what_it_is_for() {
    let _guard = one_at_a_time();
    let machine = Machine::bare()
        .with_tool("cargo", "cargo 1.97.1 (1d8b05cdd 2026-07-20)")
        .with_tool("rustc", "rustc 1.97.1 (82e1608df 2026-07-21)")
        .with_tool("node", "v22.12.0")
        .with_tool("npm", "10.9.0")
        .with_tool("git", "git version 2.39.5 (Apple Git-154)")
        .with(
            "make",
            "echo 'GNU Make 4.4.1'; echo 'Built for x86_64-pc-linux-gnu'",
        )
        .with(
            "gcc",
            "echo 'Apple clang version 15.0.0 (clang-1500.1.0.2.5)'; echo 'Target: arm64-apple-darwin23.1.0'",
        )
        .with(
            "g++",
            "echo 'Ubuntu clang version 18.1.3 (1ubuntu1)'; echo 'Target: x86_64-pc-linux-gnu'",
        )
        .with(
            "pkg-config",
            "case \"$1 $2\" in '--version '*) echo '0.29.2' ;; '--modversion openssl') echo '3.0.13' ;; *) exit 1 ;; esac",
        )
        .with(
            "cmake",
            "echo 'cmake version 3.28.1'; echo; echo 'CMake suite maintained and supported by Kitware (kitware.com/cmake).'",
        )
        .with_tool("python3", "Python 3.12.1");
    let data = tempfile::tempdir().expect("a data root");
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");

    let out = cmd(env!("CARGO_BIN_EXE_gglib"))
        .args(["config", "check-deps"])
        .current_dir(elsewhere.path())
        .env_clear()
        .env("PATH", machine.path())
        .env("HOME", data.path())
        .env("GGLIB_DATA_DIR", data.path())
        .env("GGLIB_RESOURCE_DIR", data.path())
        .stdin(Stdio::null())
        .output()
        .expect("running `gglib config check-deps`");

    let calls = machine.calls();
    for tool in ["cargo", "git", "cmake", "g++", "python3"] {
        assert!(
            calls.contains(&format!("{tool} --version")),
            "{tool} is asked its version: {calls:?}"
        );
    }

    let stdout = plain(&String::from_utf8_lossy(&out.stdout));
    let rows_under = |title: &str, count: usize| -> Vec<String> {
        stdout
            .lines()
            .skip_while(|line| *line != title)
            .skip(2)
            .take(count)
            .map(str::to_owned)
            .collect()
    };
    let explain = format!(
        "stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Every state and every note starts in one column for every group: the
    // first after the widest name (` stable-diffusion.cpp`) and the widest
    // state.
    assert_eq!(
        rows_under("Building gglib from source", 4),
        [
            "*cargo                ✓ v1.97.1       Builds gglib's Rust code",
            "*rustc                ✓ v1.97.1       The Rust compiler",
            "*node                 ✓ v22.12.0      Builds the web UI and the desktop app",
            "*npm                  ✓ v10.9.0       Installs the web UI's packages",
        ],
        "{explain}"
    );
    assert_eq!(
        rows_under("Building an engine from source", 6),
        [
            "*git                  ✓ v2.39.5       Fetches an engine's source",
            "*cmake                ✓ v3.28.1       Configures an engine's build",
            "*make                 ✓ v4.4.1        Runs the build cmake writes",
            "*C++ compiler         ✓ v18.1.3       Compiles an engine",
            "*pkg-config           ✓ v0.29.2       Finds the system libraries an engine links",
            "*libssl-dev           ✓ v3.0.13       llama.cpp's HTTPS support (development files)",
        ],
        "{explain}"
    );
    assert!(
        stdout.contains(
            " python3              ✓ v3.12.1       Turns on the optional hf_xet download accelerator"
        ),
        "{explain}"
    );
}

/// A tool that is not there is a missing row, and one that fails when asked
/// its version is as good as not there.
#[test]
fn check_deps_reports_a_tool_that_is_absent_or_broken_as_missing() {
    let _guard = one_at_a_time();
    let machine = Machine::bare()
        .with_tool("cargo", "cargo 1.97.1 (1d8b05cdd 2026-07-20)")
        .with("git", "echo 'git version 2.43.0'; exit 1");
    let data = tempfile::tempdir().expect("a data root");
    let elsewhere = tempfile::tempdir().expect("a working directory apart from the data root");

    let out = cmd(env!("CARGO_BIN_EXE_gglib"))
        .args(["config", "check-deps"])
        .current_dir(elsewhere.path())
        .env_clear()
        .env("PATH", machine.path())
        .env("HOME", data.path())
        .env("GGLIB_DATA_DIR", data.path())
        .env("GGLIB_RESOURCE_DIR", data.path())
        .stdin(Stdio::null())
        .output()
        .expect("running `gglib config check-deps`");

    let stdout = plain(&String::from_utf8_lossy(&out.stdout));
    assert!(!out.status.success(), "{stdout}");
    assert!(
        stdout.contains("*cargo                ✓ v1.97.1       Builds gglib's Rust code"),
        "{stdout}"
    );
    for missing in ["*rustc ", "*git ", "*cmake "] {
        assert!(
            stdout
                .lines()
                .any(|line| line.starts_with(missing) && line.contains("✗ missing")),
            "{missing} is missing in\n{stdout}"
        );
    }
}
