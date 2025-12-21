//! Graceful shutdown logic for `tokio::process::Child` with SIGTERM → SIGKILL escalation.

use std::io;
use std::process::ExitStatus;

use tokio::process::Child;

#[cfg(unix)]
use std::time::Duration;
#[cfg(unix)]
use tokio::time::timeout;

#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

/// Gracefully shut down a child process with SIGTERM, escalating to SIGKILL if needed.
///
/// # Strategy
/// 1. Send SIGTERM and wait up to 5 seconds for graceful exit
/// 2. If still running, send SIGKILL
/// 3. Wait for process reaping (required to avoid zombies)
///
/// # Platform behavior
/// - Unix: Uses nix crate for SIGTERM, then SIGKILL via `.kill()`
/// - Windows: Immediately calls `.kill()` (no graceful shutdown available)
///
/// # Returns
/// - `Ok(ExitStatus)` once the process has been reaped
/// - `Err` if process operations fail
pub async fn shutdown_child(mut child: Child) -> io::Result<ExitStatus> {
    #[cfg(unix)]
    {
        shutdown_unix(&mut child).await
    }

    #[cfg(not(unix))]
    {
        shutdown_windows(&mut child).await
    }
}

#[cfg(unix)]
async fn shutdown_unix(child: &mut Child) -> io::Result<ExitStatus> {
    let pid = child
        .id()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "child has no PID"))?;

    // Phase 1: SIGTERM with 5-second grace period
    if let Err(e) = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
        // Process may have already exited
        if e == nix::errno::Errno::ESRCH {
            return child.wait().await;
        }
        return Err(io::Error::other(e));
    }

    // Wait up to 5 seconds for graceful exit
    match timeout(Duration::from_secs(5), child.wait()).await {
        Ok(result) => return result,
        Err(_) => {
            // Timeout - escalate to SIGKILL
        }
    }

    // Phase 2: SIGKILL (via Child::kill which uses SIGKILL on Unix)
    child.kill().await?;

    // Phase 3: Wait for reaping (should be fast after SIGKILL)
    child.wait().await
}

#[cfg(not(unix))]
async fn shutdown_windows(child: &mut Child) -> io::Result<ExitStatus> {
    // Windows has no SIGTERM equivalent - terminate immediately
    child.kill().await?;
    child.wait().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::process::Command;
    use tokio::time::sleep;

    #[tokio::test]
    #[cfg(unix)]
    async fn shutdown_responds_to_sigterm() {
        // Spawn sleep process that should respond to SIGTERM
        let child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("failed to spawn sleep");

        let result = shutdown_child(child).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn shutdown_handles_already_exited() {
        // Spawn process that exits immediately
        let child = Command::new("echo")
            .arg("test")
            .spawn()
            .expect("failed to spawn echo");

        // Give it time to exit
        sleep(Duration::from_millis(100)).await;

        let result = shutdown_child(child).await;
        assert!(result.is_ok());
    }
}
