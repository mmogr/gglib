//! Kill orphaned processes by PID without reaping (no Child handle available).

use std::io;
use std::time::Duration;

use tokio::time::sleep;

#[cfg(unix)]
use nix::errno::Errno;
#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

/// Kill an orphaned process by PID: SIGTERM → SIGKILL escalation on Unix, a
/// hard kill on Windows.
///
/// # Strategy on Unix
/// 1. Send SIGTERM
/// 2. Poll for up to 2 seconds to verify process exit
/// 3. If still alive, send SIGKILL
/// 4. Poll again for up to 2 seconds to verify exit
///
/// On Windows there is no SIGTERM step: `taskkill /F` from the start, then
/// the same 2-second poll (see `kill_pid_windows`).
///
/// # Differences from `shutdown_child`
/// - No `Child` handle, so **cannot reap** the process
/// - Caller must verify PID exists before calling
/// - Used for cleaning up orphaned servers from previous crashes
///
/// # Returns
/// - `Ok(())` if process was killed or already gone
/// - `Err` if kill operations fail (excluding ESRCH), or the process is still
///   there after the last poll
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

/// The pid `kill(2)` should be given for `pid`, or `None` when `pid` names no
/// single process.
///
/// `kill` reads `0` as the caller's own process group and a negative pid as a
/// group, `-1` as every process the user can signal. A `u32` above
/// `i32::MAX` would become one of those through an `as` cast, so it is
/// refused here instead.
#[cfg(unix)]
pub(crate) fn signal_target(pid: u32) -> Option<Pid> {
    i32::try_from(pid)
        .ok()
        .filter(|&raw| raw > 0)
        .map(Pid::from_raw)
}

#[cfg(unix)]
async fn kill_pid_unix(pid: u32) -> io::Result<()> {
    let nix_pid = signal_target(pid).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{pid} is not a process id"),
        )
    })?;

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
        // ESRCH: the process exited. Anything else is still alive, or
        // another error (permission) - assume still alive, and continue
        // polling.
        if signal::kill(nix_pid, None) == Err(Errno::ESRCH) {
            return Ok(());
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

        // Anything but ESRCH is still alive (very unusual after SIGKILL), or
        // another error - continue polling
        if signal::kill(nix_pid, None) == Err(Errno::ESRCH) {
            return Ok(());
        }
    }

    // If we get here, process didn't exit even after SIGKILL (rare)
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("process {pid} did not exit after SIGKILL"),
    ))
}

/// A hard kill, then up to 2 seconds of polling for the process to go.
///
/// `sysinfo::Process::kill` runs `taskkill /PID <pid> /F`, which terminates
/// the process at once: nothing is asked first, and the process gets no
/// chance to clean up. A kill that reports failure is not an error if the
/// process has gone anyway, since it may have exited on its own meanwhile.
#[cfg(not(unix))]
async fn kill_pid_windows(pid: u32) -> io::Result<()> {
    use crate::pidfile::pid_exists;
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

    // Scoped so no `System` is held across an await.
    let killed = {
        let target = sysinfo::Pid::from_u32(pid);
        let mut sys = System::new();
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[target]),
            true,
            ProcessRefreshKind::nothing(),
        );
        match sys.process(target) {
            Some(process) => process.kill(),
            None => return Ok(()),
        }
    };
    if !killed {
        if pid_exists(pid) {
            return Err(io::Error::other(format!(
                "taskkill /F could not stop process {pid}"
            )));
        }
        return Ok(());
    }

    for _ in 0..20 {
        sleep(Duration::from_millis(100)).await;
        if !pid_exists(pid) {
            return Ok(());
        }
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("process {pid} did not exit after taskkill /F"),
    ))
}

// Gated on `unix` as well as `test`: `kill_pid_terminates_process` spawns
// `sleep`, which Windows does not have. Gating the module rather than each
// test is what keeps the imports from reading as unused there.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tokio::process::Command;

    #[tokio::test]
    async fn kill_pid_handles_already_gone() {
        // Use a PID that's very unlikely to exist
        let result = kill_pid(999_999).await;
        assert!(result.is_ok());
    }

    /// `u32::MAX` would be `-1`, every process the user can signal, and `0`
    /// this process's own group: both are refused before any signal is sent.
    /// (Were either sent, this test process would get the SIGTERM.)
    #[tokio::test]
    async fn kill_pid_refuses_a_value_that_names_a_group() {
        for pid in [0, 1 << 31, u32::MAX] {
            let err = kill_pid(pid).await.expect_err("must be refused");
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{pid}");
        }
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
            eprintln!("kill_pid failed: {e}");
        }

        // Reap the child to clean up zombie
        // In real orphan cleanup, the init process (PID 1) reaps orphans
        let _ = child.wait().await;

        // After reaping, verify process is truly gone
        assert!(!crate::pidfile::pid_exists(pid));
    }
}
