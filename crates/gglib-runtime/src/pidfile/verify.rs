//! Process verification to ensure PIDs belong to llama-server.

#[cfg(any(target_os = "macos", target_os = "linux"))]
use gglib_core::paths::llama_server_path;

#[cfg(target_os = "macos")]
use sysinfo::System;

#[cfg(target_os = "linux")]
use std::fs;

/// Check if a PID belongs to our llama-server binary.
///
/// # Platform behavior
/// - **macOS**: Uses `sysinfo` to check executable path
/// - **Linux**: Reads `/proc/<pid>/exe` symlink
/// - **Other**: Always returns `false` (conservative)
///
/// # Safety
/// Returns `false` if verification fails or PID doesn't match our binary.
/// This prevents accidentally killing unrelated processes with reused PIDs.
pub fn is_our_llama_server(pid: u32) -> bool {
    #[cfg(target_os = "macos")]
    {
        is_our_llama_server_macos(pid)
    }

    #[cfg(target_os = "linux")]
    {
        is_our_llama_server_linux(pid)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = pid;
        false
    }
}

#[cfg(target_os = "macos")]
fn is_our_llama_server_macos(pid: u32) -> bool {
    let Ok(expected_path) = llama_server_path() else {
        return false;
    };

    // Use new_all() to ensure processes are loaded
    let sys = System::new_all();

    let Some(process) = sys.process(sysinfo::Pid::from_u32(pid)) else {
        return false;
    };

    let Some(exe_path) = process.exe() else {
        return false;
    };

    // Compare canonical paths
    match (exe_path.canonicalize(), expected_path.canonicalize()) {
        (Ok(actual), Ok(expected)) => actual == expected,
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn is_our_llama_server_linux(pid: u32) -> bool {
    let Ok(expected_path) = llama_server_path() else {
        return false;
    };

    let proc_exe = format!("/proc/{}/exe", pid);
    let Ok(actual_path) = fs::read_link(&proc_exe) else {
        return false;
    };

    // Compare canonical paths
    match (actual_path.canonicalize(), expected_path.canonicalize()) {
        (Ok(actual), Ok(expected)) => actual == expected,
        _ => false,
    }
}

/// Check if a PID exists (without verifying it's our process).
///
/// Uses `kill` with null signal which doesn't send a signal but checks existence.
#[cfg(unix)]
pub fn pid_exists(pid: u32) -> bool {
    use nix::sys::signal;
    use nix::unistd::Pid;

    // Signal None is a special "null signal" that checks if we can signal the process
    match signal::kill(Pid::from_raw(pid as i32), None) {
        Ok(_) => true,
        Err(nix::errno::Errno::ESRCH) => false, // No such process
        Err(_) => true,                         // Process exists but we lack permission
    }
}

#[cfg(not(unix))]
pub fn pid_exists(_pid: u32) -> bool {
    false // Not implemented on non-Unix
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
    fn pid_exists_false_for_impossible_pid() {
        assert!(!pid_exists(999999));
    }

    #[test]
    fn is_our_llama_server_false_for_self() {
        // Current process is not llama-server
        let self_pid = std::process::id();
        assert!(!is_our_llama_server(self_pid));
    }
}
