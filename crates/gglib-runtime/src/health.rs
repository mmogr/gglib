//! Health check utilities for llama-server processes.
//!
//! This module provides HTTP health checking for server processes.
//! It is intentionally minimal and has no domain logic.
#![allow(dead_code)] // Utility functions may not all be used yet

use anyhow::Result;
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info};

/// Shared client for health polling.
///
/// Built once and reused. `reqwest::Client` construction is not cheap — it
/// initializes a TLS backend and loads the system root certificate store —
/// and health checks are the most frequently repeated request in the process
/// (`ServerHealthMonitor` polls on an interval, and `wait_for_http_health`
/// polls in a loop during every model start). Constructing one per call
/// discarded the connection pool each time and, under load, could take longer
/// than the request it was built for.
static HEALTH_CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();

/// The shared health-check client, or an error if it could not be built.
///
/// Deliberately keeps the fallible signature rather than panicking: client
/// construction can fail on a misconfigured TLS backend, and a health check is
/// exactly the code path that should report that rather than abort the process.
fn health_client() -> Result<&'static Client> {
    if let Some(client) = HEALTH_CLIENT.get() {
        return Ok(client);
    }
    let client = Client::builder().timeout(Duration::from_secs(2)).build()?;
    // A concurrent caller may have won the race; either instance is equivalent.
    let _ = HEALTH_CLIENT.set(client);
    Ok(HEALTH_CLIENT
        .get()
        .expect("HEALTH_CLIENT set above or by a concurrent caller"))
}

/// Check HTTP health of a server at the given port.
///
/// Makes a single request to the health endpoint and returns
/// whether the server responded successfully.
pub async fn check_http_health(port: u16) -> Result<bool> {
    let health_url = format!("http://127.0.0.1:{}/health", port);

    match health_client()?.get(&health_url).send().await {
        Ok(response) if response.status().is_success() => Ok(true),
        Ok(_) => Ok(false),
        Err(_) => Ok(false),
    }
}

/// Wait for HTTP health check to succeed.
///
/// Polls the llama-server's /health endpoint until it returns 200 OK
/// or the timeout is reached.
///
/// # Arguments
///
/// * `port` - Port the server is listening on
/// * `timeout_secs` - Maximum seconds to wait
pub async fn wait_for_http_health(port: u16, timeout_secs: u64) -> Result<()> {
    let health_url = format!("http://127.0.0.1:{}/health", port);
    info!("Waiting for llama-server to be ready at {}", health_url);

    let max_attempts = timeout_secs;
    let mut attempt = 0;
    let client = Client::builder().timeout(Duration::from_secs(2)).build()?;

    loop {
        attempt += 1;
        sleep(Duration::from_secs(1)).await;

        match client.get(&health_url).send().await {
            Ok(response) => {
                let status = response.status();

                if !status.is_success() {
                    debug!(
                        "Health check returned status {} (expected 200), retrying...",
                        status
                    );

                    // Fail faster if clearly wrong service
                    if (status.as_u16() == 403 || status.as_u16() == 404) && attempt > 3 {
                        return Err(anyhow::anyhow!(
                            "Port {} appears to be in use by another service (status {})",
                            port,
                            status
                        ));
                    }
                } else {
                    // Got 200 OK - verify it's actually llama-server
                    match response.text().await {
                        Ok(body) => {
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
                "llama-server failed to start within {}s on port {}",
                max_attempts,
                port
            ));
        }
    }
}

/// Check if a process is alive using its PID.
///
/// Uses a simple file-based check on Unix systems.
#[cfg(unix)]
pub fn check_process_alive(pid: u32) -> bool {
    // Check if /proc/<pid> exists (Linux) or use kill signal check
    std::path::Path::new(&format!("/proc/{}", pid)).exists()
        || std::fs::metadata(format!("/proc/{}", pid)).is_ok()
}

#[cfg(not(unix))]
pub fn check_process_alive(_pid: u32) -> bool {
    // Windows/other: assume alive if we have a PID
    // Full implementation would use platform-specific APIs
    true
}
