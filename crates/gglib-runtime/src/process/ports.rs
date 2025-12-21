//! Port allocation utilities for process management.

use anyhow::{Result, anyhow};
use std::net::TcpListener;
use tracing::debug;

/// Check if a port is available by attempting to bind to it.
/// This method binds and immediately drops the listener, which releases the port.
pub fn is_port_available(port: u16) -> bool {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            // Get the actual bound address to ensure it worked
            listener.local_addr().is_ok()
        }
        Err(_) => false,
    }
}

/// Allocate an available port from a range, avoiding ports already in use.
pub fn allocate_port(base_port: u16, used_ports: &[u16]) -> Result<u16> {
    // Try multiple times with small delays to handle race conditions
    for attempt in 0..3 {
        for offset in 0..100 {
            let port = base_port + offset;

            // Skip ports we're already tracking
            if used_ports.contains(&port) {
                continue;
            }

            // Check if port is actually available
            if is_port_available(port) {
                debug!(
                    port = %port,
                    attempt = %(attempt + 1),
                    "Allocated available port"
                );

                // Double-check availability immediately before returning
                std::thread::sleep(std::time::Duration::from_millis(10));
                if is_port_available(port) {
                    return Ok(port);
                } else {
                    debug!(port = %port, "Port became unavailable, retrying");
                }
            } else {
                debug!(port = %port, "Port unavailable on system, skipping");
            }
        }

        // Small delay between attempts
        if attempt < 2 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    Err(anyhow!(
        "No available ports in range {}-{} after 3 attempts",
        base_port,
        base_port + 99
    ))
}
