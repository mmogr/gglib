//! Health check utilities for llama-server processes.
//!
//! This module provides HTTP health checking for server processes.
//! It is intentionally minimal and has no domain logic.
#![allow(dead_code)] // Utility functions may not all be used yet

use anyhow::Result;
use reqwest::Client;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::sleep;
use tracing::{debug, info};

use crate::command::CapturedOutput;

/// Check HTTP health of a server at the given port.
///
/// Makes a single request to the health endpoint and returns
/// whether the server responded successfully.
pub async fn check_http_health(port: u16) -> Result<bool> {
    let health_url = format!("http://127.0.0.1:{}/health", port);
    let client = Client::builder().timeout(Duration::from_secs(2)).build()?;

    match client.get(&health_url).send().await {
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

/// Wait for HTTP health check to succeed, aborting early if the process exits.
///
/// This is the fast-failing variant of [`wait_for_http_health`]. It accepts a
/// [`oneshot::Receiver`] that fires when llama-server's stderr pipe closes
/// (produced by [`crate::command::spawn_with_exit_watch`]).  When the receiver
/// fires, the loop aborts immediately and returns a descriptive error containing
/// the captured output from the process, rather than waiting for `timeout_secs`.
///
/// # Arguments
///
/// * `port` — Port the server is expected to listen on
/// * `timeout_secs` — Maximum seconds to wait before giving up
/// * `exit_rx` — Fires with captured stdout/stderr when the process exits
pub async fn wait_for_http_health_or_exit(
    port: u16,
    timeout_secs: u64,
    exit_rx: oneshot::Receiver<CapturedOutput>,
) -> Result<()> {
    let health_url = format!("http://127.0.0.1:{}/health", port);
    info!("Waiting for llama-server to be ready at {}", health_url);

    let max_attempts = timeout_secs;
    let mut attempt = 0u64;
    let client = Client::builder().timeout(Duration::from_secs(2)).build()?;

    // Pin the exit receiver so we can use it in select! repeatedly.
    let mut exit_rx = exit_rx;

    loop {
        attempt += 1;

        // Wait 1 second, but abort early if the process exits.
        tokio::select! {
            biased;

            // Process exited — fail fast with its captured output.
            captured = &mut exit_rx => {
                let context = captured
                    .unwrap_or_default()
                    .best_effort_context();
                return Err(anyhow::anyhow!(
                    "llama-server exited unexpectedly before becoming ready.\n\nOutput:\n{}",
                    context
                ));
            }

            _ = sleep(Duration::from_secs(1)) => {}
        }

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
