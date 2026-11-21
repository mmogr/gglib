//! Graceful shutdown logic for `tokio::process::Child` with SIGTERM → SIGKILL escalation.

use std::future::Future;
use std::io;
use std::process::ExitStatus;

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
/// indefinite hang by returning after `secs` with a `TimedOut` error.
///
/// Note: when this function returns due to timeout, the Tokio `Child` struct
/// is dropped by the caller. Tokio spawns a background reaper task to await
/// the zombie process asynchronously — the reap is not lost, just detached
/// from the blocking shutdown path.
async fn bounded_wait<F>(fut: F, secs: u64, pid: Option<u32>) -> io::Result<ExitStatus>
where
    F: Future<Output = io::Result<ExitStatus>>,
{
    timeout(Duration::from_secs(secs), fut)
        .await
        .unwrap_or_else(|_| {
            let pid_str = pid.map_or_else(|| "unknown".to_string(), |p| p.to_string());
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
        })
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
#[expect(
    clippy::cast_possible_wrap,
    reason = "the pid is the OS's own, whose pid_t is i32, so it fits"
)]
async fn shutdown_unix(child: &mut Child) -> io::Result<ExitStatus> {
    // Phase 1: SIGTERM with platform-specific grace period
    // Linux: 2 seconds (faster shutdown for better UX)
    // Other Unix: 5 seconds (more conservative)
    #[cfg(target_os = "linux")]
    const SIGTERM_TIMEOUT_SECS: u64 = 2;
    #[cfg(not(target_os = "linux"))]
    const SIGTERM_TIMEOUT_SECS: u64 = 5;

    let pid = child
        .id()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "child has no PID"))?;

    if let Err(e) = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM) {
        // Process may have already exited
        if e == nix::errno::Errno::ESRCH {
            return child.wait().await;
        }
        return Err(io::Error::other(e));
    }

    // Wait for graceful exit
    if let Ok(result) = timeout(Duration::from_secs(SIGTERM_TIMEOUT_SECS), child.wait()).await {
        return result;
    }
    // Timeout - escalate to SIGKILL

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
    use gglib_core::utils::process::async_cmd;
    use tokio::time::sleep;

    /// A running child is stopped, well inside the time it would have run
    /// for, and is gone afterwards.
    #[tokio::test]
    async fn shutdown_stops_a_running_child() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let program = gglib_test_proc::install(dir.path(), "llama-server", "sleep 60");
        let child = async_cmd(&program)
            .kill_on_drop(true)
            .spawn()
            .expect("the stand-in starts");
        let pid = child.id().expect("a running child has a pid");
        let started = std::time::Instant::now();

        let status = shutdown_child(child).await.expect("the child is stopped");

        assert!(!status.success(), "it was stopped, not left to finish");
        assert!(started.elapsed() < Duration::from_secs(30));
        assert!(!crate::pidfile::pid_exists(pid));
    }

    /// A child that has already exited is waited for, and its own exit
    /// status is what comes back.
    #[tokio::test]
    async fn shutdown_handles_already_exited() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let done = dir.path().join("done");
        let program = gglib_test_proc::install(
            dir.path(),
            "llama-server",
            &format!("touch {}", done.display()),
        );
        let child = async_cmd(&program).spawn().expect("the stand-in starts");

        // The file is the last thing the stand-in does; then give it time
        // to exit.
        for _ in 0..300 {
            if done.exists() {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
        assert!(done.exists(), "the stand-in never ran to its end");
        sleep(Duration::from_millis(200)).await;

        let status = shutdown_child(child)
            .await
            .expect("an exited child is waited for");
        assert!(status.success(), "{status}");
    }

    #[tokio::test(start_paused = true)]
    async fn bounded_wait_times_out_on_pending_future() {
        use std::future::pending;
        use tokio::time::{Duration as TokioDuration, advance};

        // Advance time past the 2-second timeout
        advance(TokioDuration::from_secs(3)).await;

        let result = bounded_wait(pending::<io::Result<ExitStatus>>(), 2, Some(999)).await;

        // Should return TimedOut error
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        let msg = err.to_string();
        assert!(
            msg.contains("pid=999"),
            "error message should contain PID: {msg}"
        );
    }

    // Unix only: a SIGTERM the process ignores, then the SIGKILL it cannot;
    // Windows has one kill.
    #[cfg(unix)]
    #[tokio::test]
    async fn shutdown_escalates_to_sigkill_when_sigterm_is_ignored() {
        // Spawn a single process that ignores SIGTERM — forces escalation to SIGKILL.
        // Uses python3 to avoid orphaning a subprocess (which bash -c 'sleep' would do).
        let child = async_cmd("python3")
            .arg("-c")
            .arg("import signal, time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(30)")
            .spawn()
            .expect("failed to spawn python3");

        let result = shutdown_child(child).await;

        // Should succeed: SIGTERM is ignored, so escalation sends SIGKILL which cannot be trapped.
        assert!(
            result.is_ok(),
            "shutdown should succeed via SIGKILL escalation: {result:?}"
        );
    }
}
