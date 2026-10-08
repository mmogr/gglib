//! `gglib config check-deps` on a fixture machine: a `PATH` of stand-ins that
//! answer `--version` with banners the real tools print.
//!
//! The rows asserted are the ones every platform lists, in the order the
//! command lists them, as it printed them before the version probes behind
//! them became one family. A platform's own rows (the libraries Linux links,
//! the GPU) follow these and are not compared: they depend on the machine.

#![cfg(unix)]

#[path = "support/stub_tools.rs"]
mod stub_tools;

use std::process::{Command, Stdio};

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
fn check_deps_prints_each_tool_as_it_always_has() {
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

    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
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
    let rows: Vec<&str> = stdout
        .lines()
        .skip_while(|line| !line.starts_with("===="))
        .skip(1)
        .take(12)
        .collect();
    assert_eq!(
        rows,
        [
            "*cargo               ✓ v1.97.1        Required for building Rust code",
            "*rustc               ✓ v1.97.1        Rust compiler",
            "*node                ✓ v22.12.0       Required for building web UI and Tauri",
            "*npm                 ✓ v10.9.0        Node package manager",
            "*git                 ✓ v2.39.5        Required for llama.cpp installation",
            "*make                ✓ v4.4.1         Required for llama.cpp build",
            "*gcc                 ✓ v15.0.0        Required for llama.cpp compilation",
            "*g++                 ✓ v18.1.3        Required for llama.cpp compilation",
            "*pkg-config          ✓ v0.29.2        Required for building with system libraries",
            "*libssl-dev          ✓ v3.0.13        Required for llama.cpp's HTTPS support",
            "*cmake               ✓ v3.28.1        Required for llama.cpp build",
            " python3             ✓ v3.12.1        Optional: enables the hf_xet download accelerator",
        ],
        "stdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
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

    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
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
        stdout.contains("*cargo               ✓ v1.97.1        Required for building Rust code"),
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
