//! Tests for [`super::ServerHealthChecker`] and [`super::ServerHealthMonitor`].
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;
use futures_util::StreamExt;
use std::time::Duration;

#[tokio::test]
async fn test_health_checker_http_unreachable() {
    // Check a port that's definitely not in use
    let status = ServerHealthChecker::check_http(65432).await;
    assert!(matches!(status, ServerHealthStatus::Unreachable { .. }));
}

#[test]
fn test_process_check_with_invalid_pid() {
    // PID 999999 should not exist
    let handle = ProcessHandle::new(1, "test".to_string(), Some(999999), 8080, 0);
    let status = ServerHealthChecker::check_process(&handle);
    assert_eq!(status, ServerHealthStatus::ProcessDied);
}

#[test]
fn test_process_check_without_pid() {
    // No PID means we can't check, should return Healthy (HTTP will catch issues)
    let handle = ProcessHandle::new(1, "test".to_string(), None, 8080, 0);
    let status = ServerHealthChecker::check_process(&handle);
    assert_eq!(status, ServerHealthStatus::Healthy);
}

#[tokio::test]
async fn test_monitor_cancellation() {
    // Create a monitor for an unused port (will be unreachable)
    let handle = ProcessHandle::new(1, "test".to_string(), None, 65433, 0);
    let cancel_token = CancellationToken::new();

    let monitor = ServerHealthMonitor::new(handle, Duration::from_millis(50), cancel_token.clone());

    let mut stream = Box::pin(monitor.monitor());

    // Cancel immediately
    cancel_token.cancel();

    // Stream should complete after cancellation
    // Give it a moment to process
    tokio::time::sleep(Duration::from_millis(100)).await;

    // The stream should not yield any more after cancellation
    // (first tick may have already happened before cancellation)
    let result = tokio::time::timeout(Duration::from_millis(200), stream.next()).await;

    // Either timed out (stream completed) or got one last item then completed
    match result {
        Ok(Some(_)) => {
            // Got one item, next should be None (stream completed)
            let next = tokio::time::timeout(Duration::from_millis(100), stream.next()).await;
            assert!(next.is_err() || next.unwrap().is_none());
        }
        Ok(None) => {} // Stream completed, good
        Err(_) => {}   // Timeout, stream is done, good
    }
}

#[tokio::test]
async fn test_monitor_emits_initial_status() {
    // Create a monitor for an unused port
    let handle = ProcessHandle::new(1, "test".to_string(), None, 65434, 0);
    let cancel_token = CancellationToken::new();

    let monitor = ServerHealthMonitor::new(handle, Duration::from_millis(10), cancel_token.clone());

    let mut stream = Box::pin(monitor.monitor());

    // Should get an initial status on first tick.
    //
    // The budget is deliberately far larger than the 10 ms poll interval:
    // this asserts *liveness* (the stream emits at all), not latency. Under
    // `cargo test --workspace` every crate's test binary runs concurrently,
    // and this test's `current_thread` runtime can wait a long time to be
    // scheduled — a tight budget here fails on contention rather than on
    // anything the monitor did wrong.
    let first_status = tokio::time::timeout(Duration::from_secs(10), stream.next()).await;

    cancel_token.cancel();

    assert!(first_status.is_ok());
    let status = first_status.unwrap();
    assert!(status.is_some());
    // Should be unreachable since nothing is listening on that port
    assert!(matches!(
        status.unwrap(),
        ServerHealthStatus::Unreachable { .. }
    ));
}

/// Regression guard for the macOS liveness bug.
///
/// Our own PID is alive by definition, so this must report `Healthy`. The
/// `cfg(unix)` arm used to read `/proc/<pid>`, which does not exist on macOS, so
/// every PID read as dead there and `check_combined` would have short-circuited
/// to `ProcessDied` without ever attempting the HTTP check.
///
/// `test_process_check_with_invalid_pid` could not catch this: with `/proc`
/// absent, *every* PID read as dead, so asserting that an invalid PID is dead
/// passed for the wrong reason.
///
/// **This guard is vacuous on CI.** The only job running `cargo test -p
/// gglib-runtime` is `test:` on `ubuntu-latest` (`.github/workflows/ci.yml`),
/// and on Linux `/proc/<self>` exists — so the pre-fix implementation passes
/// this too. It fails only where `/proc` is absent, i.e. on a macOS developer
/// machine. Do not read its green status on CI as protection; if the delegation
/// in `is_process_alive` is ever reverted, only a local macOS run will notice.
#[test]
fn a_live_process_reports_healthy_rather_than_dead() {
    let handle = ProcessHandle::new(1, "test".to_string(), Some(std::process::id()), 8080, 0);
    let status = ServerHealthChecker::check_process(&handle);
    assert_eq!(status, ServerHealthStatus::Healthy);
}
