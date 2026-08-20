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
    ///
    /// Delegates to [`crate::pidfile::pid_exists`], which asks the kernel with a
    /// null signal instead of reading `/proc`. This function used to do the
    /// latter under `cfg(unix)` — but macOS is `cfg(unix)` and has no `/proc`, so
    /// on macOS every live `llama-server` *would* have read as dead, and
    /// [`Self::check_combined`] would have returned `ProcessDied` without ever
    /// attempting the HTTP check.
    ///
    /// The bug was latent rather than observed: the only production caller of
    /// [`ServerHealthMonitor`] builds its `ProcessHandle` with `pid: None`
    /// (`gglib-app-services/src/servers.rs`), and [`Self::check_process`] returns
    /// `Healthy` on that branch without consulting this function at all. Supplying
    /// a real PID — which the public API permits — was all it would have taken.
    #[cfg(unix)]
    fn is_process_alive(pid: u32) -> bool {
        crate::pidfile::pid_exists(pid)
    }

    /// On non-Unix we cannot check cheaply, so assume alive and let the HTTP
    /// check detect failures.
    ///
    /// Deliberately **not** delegating to `pidfile::pid_exists`: its
    /// `cfg(not(unix))` arm returns `false` ("not implemented"), which would
    /// report every Windows server *that supplied a PID* as `ProcessDied` —
    /// reintroducing there the exact bug this change removes from macOS.
    /// `x86_64-pc-windows-msvc` is a release target.
    #[cfg(not(unix))]
    fn is_process_alive(_pid: u32) -> bool {
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
#[path = "health_monitor_tests.rs"]
mod health_monitor_tests;
