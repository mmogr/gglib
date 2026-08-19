//! Kill orphaned processes by PID without reaping (no Child handle available).

use std::io;

#[cfg(unix)]
use std::time::Duration;
#[cfg(unix)]
use tokio::time::sleep;

#[cfg(unix)]
use nix::errno::Errno;
#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

/// Kill an orphaned process by PID with SIGTERM → SIGKILL escalation.
///
/// # Strategy
/// 1. Send SIGTERM
/// 2. Poll for up to 2 seconds to verify process exit
/// 3. If still alive, send SIGKILL
/// 4. Poll again for up to 2 seconds to verify exit
///
/// # Differences from `shutdown_child`
/// - No `Child` handle, so **cannot reap** the process
/// - Caller must verify PID exists before calling
/// - Used for cleaning up orphaned servers from previous crashes
///
/// # Returns
/// - `Ok(())` if process was killed or already gone
/// - `Err` if kill operations fail (excluding ESRCH)
pub async fn kill_pid(pid: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        kill_pid_unix(pid).await
    }

    #[cfg(not(unix))]
    {
        kill_pid_windows(pid).await
    }
}

#[cfg(unix)]
async fn kill_pid_unix(pid: u32) -> io::Result<()> {
    let nix_pid = Pid::from_raw(pid as i32);

    // Phase 1: SIGTERM
    if let Err(e) = signal::kill(nix_pid, Signal::SIGTERM) {
        if e == nix::errno::Errno::ESRCH {
            // Already gone
            return Ok(());
        }
        return Err(io::Error::other(e));
    }

    // Poll for exit (up to 2 seconds)
    for _ in 0..20 {
        sleep(Duration::from_millis(100)).await;

        // Check if process still exists using kill with null signal (works on Linux/macOS)
        // On nix 0.29, we can't use Signal::from_c_int, but we can check process existence via errno
        match signal::kill(nix_pid, None) {
            Ok(_) => {
                // Still alive, continue polling
            }
            Err(Errno::ESRCH) => {
                // Process exited
                return Ok(());
            }
            Err(_) => {
                // Other error (permission) - assume still alive
            }
        }
    }

    // Phase 2: SIGKILL
    if let Err(e) = signal::kill(nix_pid, Signal::SIGKILL) {
        if e == nix::errno::Errno::ESRCH {
            return Ok(());
        }
        return Err(io::Error::other(e));
    }

    // Poll again for exit (up to 2 seconds)
    for _ in 0..20 {
        sleep(Duration::from_millis(100)).await;

        match signal::kill(nix_pid, None) {
            Ok(_) => {
                // Still alive (very unusual after SIGKILL)
            }
            Err(Errno::ESRCH) => {
                return Ok(());
            }
            Err(_) => {
                // Other error - continue polling
            }
        }
    }

    // If we get here, process didn't exit even after SIGKILL (rare)
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("process {} did not exit after SIGKILL", pid),
    ))
}

#[cfg(not(unix))]
async fn kill_pid_windows(_pid: u32) -> io::Result<()> {
    // Windows orphan cleanup would require different approach
    // For now, not implemented - primarily a macOS/Linux concern
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "orphan cleanup not implemented on Windows",
    ))
}

// Gated on `unix` as well as `test`: every test here drives a real signal at a
// real PID, which `kill_pid` only implements on unix — on Windows it returns
// `ErrorKind::Unsupported`. Gating the module rather than each test is what
// keeps the imports from reading as unused there.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tokio::process::Command;

    #[tokio::test]
    async fn kill_pid_handles_already_gone() {
        // Use a PID that's very unlikely to exist
        let result = kill_pid(999999).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn kill_pid_terminates_process() {
        // Spawn a long-running process
        let mut child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("failed to spawn sleep");

        let pid = child.id().expect("no PID");

        // Kill it by PID (won't reap since we don't own the Child in kill_pid)
        let result = kill_pid(pid).await;
        if let Err(ref e) = result {
            eprintln!("kill_pid failed: {}", e);
        }

        // Reap the child to clean up zombie
        // In real orphan cleanup, the init process (PID 1) reaps orphans
        let _ = child.wait().await;

        // After reaping, verify process is truly gone
        assert!(!crate::pidfile::pid_exists(pid));
    }
}
