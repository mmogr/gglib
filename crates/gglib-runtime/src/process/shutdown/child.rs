//! Graceful shutdown logic for `tokio::process::Child` with SIGTERM → SIGKILL escalation.

use std::io;
use std::process::ExitStatus;
use std::future::Future;

use tokio::process::Child;

use std::time::Duration;
use tokio::time::timeout;

#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

const SIGKILL_REAP_TIMEOUT_SECS: u64 = 2;

/// Wait for a future with a bounded timeout, logging an error on expiry.
///
/// Used after SIGKILL to reap the child process. If the process is stuck in
/// D-state (e.g., CUDA driver ioctl blocked in kernel), this prevents an
/// indefinite hang by returning after `secs` with a TimedOut error.
///
/// Note: when this function returns due to timeout, the Tokio `Child` struct
/// is dropped by the caller. Tokio spawns a background reaper task to await
/// the zombie process asynchronously — the reap is not lost, just detached
/// from the blocking shutdown path.
async fn bounded_wait<F>(fut: F, secs: u64, pid: Option<u32>) -> io::Result<ExitStatus>
where
    F: Future<Output = io::Result<ExitStatus>>,
{
    match timeout(Duration::from_secs(secs), fut).await {
        Ok(result) => result,
        Err(_) => {
            let pid_str = pid.map_or("unknown".to_string(), |p| p.to_string());
            tracing::error!(
                timeout_secs = secs,
                pid = %pid_str,
                "Process did not exit within bounded wait after SIGKILL; \
                 it may be stuck in D-state (e.g., blocked CUDA driver ioctl). \
                 Resources (port, GPU memory) may remain held. Proceeding with cleanup."
            );
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("child did not exit within bounded wait after SIGKILL: pid={pid_str}"),
            ))
        }
    }
}

/// Gracefully shut down a child process with SIGTERM, escalating to SIGKILL if needed.
///
/// # Strategy
/// 1. Send SIGTERM and wait for graceful exit (2s on Linux, 5s on other Unix)
/// 2. If still running, send SIGKILL
/// 3. Wait for process reaping with a bounded timeout (guards against D-state hang)
///
/// # Platform behavior
/// - Unix: Uses nix crate for SIGTERM, then SIGKILL via `.kill()`
/// - Windows: Immediately calls `.kill()` (no graceful shutdown available)
///
/// # Residual risk
/// If the process enters D-state (uninterruptible sleep) after SIGKILL — e.g., due to
/// a blocked CUDA driver ioctl — the bounded wait will expire and this function returns
/// an error. The caller proceeds with cleanup; resources (port, GPU memory) may remain
/// held by the stuck process. Tokio's `Child` drop handler spawns a background reaper
/// task, so the zombie is eventually collected asynchronously.
///
/// # Returns
/// - `Ok(ExitStatus)` once the process has been reaped
/// - `Err` with `io::ErrorKind::TimedOut` if the post-SIGKILL wait exceeds the bound
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

    // Phase 1: SIGTERM with platform-specific grace period
    // Linux: 2 seconds (faster shutdown for better UX)
    // Other Unix: 5 seconds (more conservative)
    #[cfg(target_os = "linux")]
    const SIGTERM_TIMEOUT_SECS: u64 = 2;
    #[cfg(not(target_os = "linux"))]
    const SIGTERM_TIMEOUT_SECS: u64 = 5;

    if let Err(e) = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
        // Process may have already exited
        if e == nix::errno::Errno::ESRCH {
            return child.wait().await;
        }
        return Err(io::Error::other(e));
    }

    // Wait for graceful exit
    match timeout(Duration::from_secs(SIGTERM_TIMEOUT_SECS), child.wait()).await {
        Ok(result) => return result,
        Err(_) => {
            // Timeout - escalate to SIGKILL
        }
    }

    // Phase 2: SIGKILL (via Child::kill which uses SIGKILL on Unix)
    child.kill().await?;

    // Phase 3: Bounded wait for reaping (guards against D-state hang)
    bounded_wait(child.wait(), SIGKILL_REAP_TIMEOUT_SECS, Some(pid)).await
}

#[cfg(not(unix))]
async fn shutdown_windows(child: &mut Child) -> io::Result<ExitStatus> {
    // Windows has no SIGTERM equivalent - terminate immediately
    let pid = child.id();
    child.kill().await?;
    bounded_wait(child.wait(), SIGKILL_REAP_TIMEOUT_SECS, pid).await
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

    #[tokio::test(start_paused = true)]
    async fn bounded_wait_times_out_on_pending_future() {
        use std::future::pending;
        use tokio::time::{advance, Duration as TokioDuration};

        // Advance time past the 2-second timeout
        advance(TokioDuration::from_secs(3)).await;

        let result = bounded_wait(pending::<io::Result<ExitStatus>>(), 2, Some(999)).await;

        // Should return TimedOut error
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        let msg = err.to_string();
        assert!(msg.contains("pid=999"), "error message should contain PID: {msg}");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn shutdown_escalates_to_sigkill_when_sigterm_is_ignored() {
        // Spawn a single process that ignores SIGTERM — forces escalation to SIGKILL.
        // Uses python3 to avoid orphaning a subprocess (which bash -c 'sleep' would do).
        let child = Command::new("python3")
            .arg("-c")
            .arg("import signal, time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(30)")
            .spawn()
            .expect("failed to spawn python3");

        let result = shutdown_child(child).await;

        // Should succeed: SIGTERM is ignored, so escalation sends SIGKILL which cannot be trapped.
        assert!(result.is_ok(), "shutdown should succeed via SIGKILL escalation: {result:?}");
    }
}
