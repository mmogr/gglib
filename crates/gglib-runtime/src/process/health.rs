//! Health check utilities for llama-server processes.

use anyhow::Result;
use sysinfo::{Pid, ProcessStatus, System};
use tokio::time::{Duration, sleep};
use tracing::{debug, info};

/// Wait for HTTP health check to succeed
///
/// Polls the llama-server's /health endpoint until it returns 200 OK
/// or the timeout is reached.
pub async fn wait_for_http_health(port: u16, timeout_secs: u64) -> Result<()> {
    let health_url = format!("http://127.0.0.1:{}/health", port);
    info!("Waiting for llama-server to be ready at {}", health_url);

    let max_attempts = timeout_secs;
    let mut attempt = 0;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()?;

    loop {
        attempt += 1;
        sleep(Duration::from_secs(1)).await;

        match client.get(&health_url).send().await {
            Ok(response) => {
                let status = response.status();

                // Only accept 200 OK - anything else is wrong
                if !status.is_success() {
                    debug!(
                        "Health check returned status {} (expected 200), retrying...",
                        status
                    );

                    // If we get a clear error from wrong service, fail faster
                    if (status.as_u16() == 403 || status.as_u16() == 404) && attempt > 3 {
                        return Err(anyhow::anyhow!(
                            "Port {} appears to be in use by another service (status {}). Try using a different port range.",
                            port,
                            status
                        ));
                    }
                } else {
                    // Got 200 OK - verify it's actually llama-server
                    match response.text().await {
                        Ok(body) => {
                            // llama-server health endpoint returns JSON with status info
                            // Check for llama-server specific content
                            if body.contains("status")
                                || body.contains("slots")
                                || body.contains("error")
                                || body.is_empty()
                            {
                                info!("llama-server is ready on port {}", port);
                                return Ok(());
                            } else {
                                debug!("Health check returned unexpected response: {}", body);
                                if attempt > 5 {
                                    return Err(anyhow::anyhow!(
                                        "Port {} is responding but doesn't appear to be llama-server",
                                        port
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            debug!("Failed to read health response: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                debug!("Health check failed: {}, retrying...", e);
            }
        }

        if attempt >= max_attempts {
            return Err(anyhow::anyhow!(
                "llama-server failed to start within {}s on port {}. Check if the port is available.",
                max_attempts,
                port
            ));
        }
    }
}

/// Single-shot HTTP health probe of a llama-server `/health` endpoint.
///
/// Unlike [`wait_for_http_health`] this does **not** retry: it makes one
/// request with a short timeout and reports whether the server responded
/// `200 OK`. Used on the "already running" fast path to detect a cached
/// server that has silently degraded or wedged, so the caller can recycle it
/// instead of routing a request into a dead instance.
///
/// Never returns an error — any failure (connection refused, timeout,
/// non-2xx) is reported as `false` so callers can treat "not healthy" and
/// "unreachable" identically.
pub async fn check_http_health(port: u16) -> bool {
    let health_url = format!("http://127.0.0.1:{port}/health");
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };

    matches!(
        client.get(&health_url).send().await,
        Ok(response) if response.status().is_success()
    )
}

/// Check if a process is alive and healthy using sysinfo
pub fn check_process_health(pid: u32) -> bool {
    let mut system = System::new_all();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, false);

    let pid = Pid::from_u32(pid);
    if let Some(process) = system.process(pid) {
        matches!(
            process.status(),
            ProcessStatus::Run | ProcessStatus::Sleep | ProcessStatus::Idle
        )
    } else {
        false
    }
}

/// Update health status for multiple processes
pub fn update_health_batch(pids: &[(u32, bool)]) -> Vec<(u32, bool)> {
    let mut system = System::new_all();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, false);

    pids.iter()
        .map(|(pid, _)| {
            let pid_val = Pid::from_u32(*pid);
            let healthy = if let Some(process) = system.process(pid_val) {
                matches!(
                    process.status(),
                    ProcessStatus::Run | ProcessStatus::Sleep | ProcessStatus::Idle
                )
            } else {
                false
            };
            (*pid, healthy)
        })
        .collect()
}
