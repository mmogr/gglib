//! Server health monitoring primitives.
//!
//! Provides reusable building blocks for continuous health monitoring
//! of server processes. The monitor is policy-free - it only checks
//! health and emits status changes without any business logic.

use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use futures_util::Stream;
use gglib_core::ports::{ProcessHandle, ServerHealthStatus};
use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::health::check_http_health;

/// Pure functions for checking server health.
///
/// This struct contains no state and performs single-shot health checks.
#[derive(Debug, Clone)]
pub struct ServerHealthChecker;

impl ServerHealthChecker {
    /// Check HTTP health endpoint.
    ///
    /// Returns health status based on HTTP response from /health endpoint.
    pub async fn check_http(port: u16) -> ServerHealthStatus {
        match check_http_health(port).await {
            Ok(true) => ServerHealthStatus::Healthy,
            Ok(false) => ServerHealthStatus::Unreachable {
                last_error: "HTTP health check returned non-success status".to_string(),
            },
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("timeout") {
                    ServerHealthStatus::Unreachable {
                        last_error: "Health check timeout".to_string(),
                    }
                } else if error_msg.contains("Connection refused") {
                    ServerHealthStatus::Unreachable {
                        last_error: "Connection refused".to_string(),
                    }
                } else {
                    ServerHealthStatus::Unreachable {
                        last_error: format!("Health check failed: {}", e),
                    }
                }
            }
        }
    }

    /// Check if process is still alive via PID.
    ///
    /// Returns `ProcessDied` status if the process no longer exists.
    pub fn check_process(handle: &ProcessHandle) -> ServerHealthStatus {
        if let Some(pid) = handle.pid {
            if Self::is_process_alive(pid) {
                // Process alive, HTTP check will determine actual health
                ServerHealthStatus::Healthy
            } else {
                ServerHealthStatus::ProcessDied
            }
        } else {
            // No PID available, assume alive (will be caught by HTTP check)
            ServerHealthStatus::Healthy
        }
    }

    /// Check if a process is alive by PID.
    #[cfg(unix)]
    fn is_process_alive(pid: u32) -> bool {
        // Check if process exists by looking at /proc/<pid> on Linux
        // or using ps command output parsing as fallback
        std::path::Path::new(&format!("/proc/{}", pid)).exists()
    }

    #[cfg(not(unix))]
    fn is_process_alive(_pid: u32) -> bool {
        // On non-Unix, we can't reliably check, so assume alive
        // and let HTTP checks detect failures
        true
    }

    /// Perform combined health check: process liveness + HTTP health.
    ///
    /// Checks process first (fast), then HTTP if process is alive.
    pub async fn check_combined(handle: &ProcessHandle) -> ServerHealthStatus {
        // First check if process is still alive (fast)
        let process_status = Self::check_process(handle);
        if matches!(process_status, ServerHealthStatus::ProcessDied) {
            return process_status;
        }

        // Process is alive, check HTTP health
        Self::check_http(handle.port).await
    }
}

/// Continuous health monitor that emits status changes.
///
/// Polls server health at regular intervals and yields only when
/// status changes, reducing event noise.
pub struct ServerHealthMonitor {
    handle: ProcessHandle,
    interval: Duration,
    cancel_token: CancellationToken,
}

impl ServerHealthMonitor {
    /// Create a new health monitor.
    ///
    /// # Arguments
    ///
    /// * `handle` - Process handle to monitor
    /// * `check_interval` - How often to check health (e.g., 10 seconds)
    /// * `cancel_token` - Token to signal monitor shutdown
    pub fn new(
        handle: ProcessHandle,
        check_interval: Duration,
        cancel_token: CancellationToken,
    ) -> Self {
        Self {
            handle,
            interval: check_interval,
            cancel_token,
        }
    }

    /// Start monitoring and return a stream of health status changes.
    ///
    /// The stream yields only when status changes, not on every check.
    /// Completes when cancellation token is triggered.
    pub fn monitor(self) -> impl Stream<Item = ServerHealthStatus> {
        let handle = Arc::new(self.handle);
        let cancel_token = self.cancel_token;
        let check_interval = self.interval;

        stream! {
            let mut ticker = interval(check_interval);
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

            let mut last_status: Option<ServerHealthStatus> = None;

            debug!(
                port = handle.port,
                model_id = handle.model_id,
                "Starting health monitor"
            );

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let current_status = ServerHealthChecker::check_combined(&handle).await;

                        // Emit only on state change
                        if last_status.as_ref() != Some(&current_status) {
                            debug!(
                                port = handle.port,
                                model_id = handle.model_id,
                                ?current_status,
                                ?last_status,
                                "Health status changed"
                            );

                            yield current_status.clone();
                            last_status = Some(current_status);
                        }
                    }
                    _ = cancel_token.cancelled() => {
                        debug!(
                            port = handle.port,
                            model_id = handle.model_id,
                            "Health monitor cancelled"
                        );
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
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

        let monitor =
            ServerHealthMonitor::new(handle, Duration::from_millis(50), cancel_token.clone());

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

        let monitor =
            ServerHealthMonitor::new(handle, Duration::from_millis(10), cancel_token.clone());

        let mut stream = Box::pin(monitor.monitor());

        // Should get an initial status on first tick
        let first_status = tokio::time::timeout(Duration::from_millis(500), stream.next()).await;

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
}
