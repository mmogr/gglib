//! Process verification: whether a pid is one of the servers gglib manages.

use gglib_core::domain::RuntimeKind;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
use gglib_core::paths::{llama_server_path, sd_server_path};
use std::path::PathBuf;

#[cfg(any(target_os = "macos", windows))]
use sysinfo::System;

#[cfg(target_os = "linux")]
use std::fs;

/// Check if a PID belongs to one of our managed server binaries:
/// llama.cpp's `llama-server` or stable-diffusion.cpp's `sd-server`.
///
/// # Platform behavior
/// - **macOS** and **Windows**: Uses `sysinfo` to check executable path
/// - **Linux**: Reads `/proc/<pid>/exe` symlink
/// - **Other**: Always returns `false` (conservative)
///
/// # Safety
/// Returns `false` if verification fails or PID doesn't match our binaries.
/// This prevents accidentally killing unrelated processes with reused PIDs.
pub fn is_our_server(pid: u32) -> bool {
    server_runtime(pid).is_some()
}

/// The runtime whose managed binary `pid` is running, verified as
/// [`is_our_server`] verifies it; `None` for any other process.
pub fn server_runtime(pid: u32) -> Option<RuntimeKind> {
    runtime_running(pid, &managed_binaries())
}

/// The binaries gglib starts servers from, one per runtime; a path that
/// cannot be resolved is left out.
fn managed_binaries() -> Vec<(RuntimeKind, PathBuf)> {
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    {
        [
            (RuntimeKind::Llama, llama_server_path()),
            (RuntimeKind::StableDiffusion, sd_server_path()),
        ]
        .into_iter()
        .filter_map(|(runtime, path)| Some((runtime, path.ok()?)))
        .collect()
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        Vec::new()
    }
}

/// The runtime of the first of `binaries` that `pid` is running, comparing
/// canonical paths. A binary that is not there matches nothing.
fn runtime_running(pid: u32, binaries: &[(RuntimeKind, PathBuf)]) -> Option<RuntimeKind> {
    let actual = executable_of(pid)?.canonicalize().ok()?;
    binaries
        .iter()
        .find(|(_, binary)| {
            binary
                .canonicalize()
                .is_ok_and(|expected| expected == actual)
        })
        .map(|(runtime, _)| *runtime)
}

#[cfg(any(target_os = "macos", windows))]
fn executable_of(pid: u32) -> Option<PathBuf> {
    // Use new_all() to ensure processes are loaded
    let sys = System::new_all();
    sys.process(sysinfo::Pid::from_u32(pid))?
        .exe()
        .map(std::path::Path::to_path_buf)
}

#[cfg(target_os = "linux")]
fn executable_of(pid: u32) -> Option<PathBuf> {
    fs::read_link(format!("/proc/{pid}/exe")).ok()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn executable_of(_pid: u32) -> Option<PathBuf> {
    None
}

/// Check if a PID exists (without verifying it's our process).
///
/// On Unix, uses `kill` with null signal which doesn't send a signal but
/// checks existence.
#[cfg(unix)]
#[allow(
    clippy::cast_possible_wrap,
    clippy::match_same_arms,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub fn pid_exists(pid: u32) -> bool {
    use nix::sys::signal;
    use nix::unistd::Pid;

    // Signal None is a special "null signal" that checks if we can signal the process
    match signal::kill(Pid::from_raw(pid as i32), None) {
        Ok(()) => true,
        Err(nix::errno::Errno::ESRCH) => false, // No such process
        Err(_) => true,                         // Process exists but we lack permission
    }
}

/// Check if a PID exists (without verifying it's our process).
///
/// Off Unix, asks `sysinfo` whether its process snapshot lists the pid.
#[cfg(not(unix))]
pub fn pid_exists(pid: u32) -> bool {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate};

    let pid = Pid::from_u32(pid);
    let mut sys = sysinfo::System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    sys.process(pid).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn pid_exists_for_self() {
        let self_pid = std::process::id();
        assert!(pid_exists(self_pid));
    }

    #[test]
    #[cfg(unix)]
    #[allow(
        clippy::unreadable_literal,
        reason = "grandfathered at lint inheritance, #1157"
    )]
    fn pid_exists_false_for_impossible_pid() {
        assert!(!pid_exists(999999));
    }

    #[test]
    fn is_our_server_false_for_self() {
        // The test binary is neither managed server
        let self_pid = std::process::id();
        assert!(!is_our_server(self_pid));
    }

    /// The sweep stops an orphan of either runtime: an `sd-server` left
    /// running holds the image model's whole footprint, ~23 GB for Flux.1.
    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn both_managed_binaries_are_ours() {
        // Before any path is resolved: another test's first call would move
        // the root between the two sides of the comparison.
        gglib_core::paths::isolate_data_root();
        assert_eq!(
            managed_binaries(),
            [
                (RuntimeKind::Llama, llama_server_path().unwrap()),
                (RuntimeKind::StableDiffusion, sd_server_path().unwrap()),
            ]
        );
    }

    /// A pid matches the binary it is running, whichever runtime's it is and
    /// wherever it sits in the list, and nothing when it runs none of them.
    /// The test binary stands in for a server: it is a running process whose
    /// path is known.
    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    fn a_pid_running_either_binary_matches_that_runtime() {
        use RuntimeKind::{Llama, StableDiffusion};

        let me = std::process::id();
        let exe = std::env::current_exe().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = dir.path().join("llama-server");
        std::fs::write(&elsewhere, b"not this process").unwrap();
        let missing = dir.path().join("sd-server");

        let sd_is_me = [(Llama, elsewhere.clone()), (StableDiffusion, exe.clone())];
        assert_eq!(runtime_running(me, &sd_is_me), Some(StableDiffusion));
        let llama_is_me = [(Llama, exe), (StableDiffusion, missing.clone())];
        assert_eq!(runtime_running(me, &llama_is_me), Some(Llama));
        let neither = [(Llama, elsewhere), (StableDiffusion, missing)];
        assert_eq!(runtime_running(me, &neither), None);
        assert_eq!(runtime_running(me, &[]), None);
    }
}
