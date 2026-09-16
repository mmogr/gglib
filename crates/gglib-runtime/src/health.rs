//! Health checking for the monitor, which needs to know *why* a check failed.
//!
//! There is a second [`check_http_health`](crate::process::check_http_health)
//! in the private `crate::process::health` module, and the difference between
//! them is deliberate rather than accidental. That one returns a bare `bool`
//! and documents that any failure — refused, timed out, non-2xx — is reported
//! identically, because its caller is the request fast path and cannot act on
//! the distinction. This one returns `Result<bool>` so
//! [`crate::health_monitor`] can turn a timeout, a refused connection and a
//! non-success status into three different `ServerHealthStatus` values.
//!
//! Collapsing them would cost the monitor its diagnosis or the fast path its
//! simplicity. What was genuinely duplicated — a `wait_for_http_health` with
//! no callers, hidden by a module-level `allow(dead_code)` — is gone.

use anyhow::Result;
use reqwest::Client;
use std::time::Duration;

/// Shared client for health polling.
///
/// Built once and reused. `reqwest::Client` construction is not cheap — it
/// initializes a TLS backend, and see [`loopback_client`] for the proxy
/// lookup it used to do as well — and health checks are the most frequently
/// repeated request in the process (`ServerHealthMonitor` polls on an
/// interval, and `wait_for_http_health` polls in a loop during every model
/// start). Constructing one per call discarded the connection pool each time
/// and, under load, could take longer than the request it was built for.
static HEALTH_CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();

/// A client for asking a server on this machine's loopback address how it is.
///
/// [`gglib_proxy::loopback::client_builder`] with a two-second timeout: that
/// builder is what makes this a loopback client rather than a general one, and
/// its docs carry the story of where a health check went with a proxy in the
/// environment (#1085) and what the system-proxy lookup cost a first check on
/// macOS (#1084). `health_proxy_tests` runs the two single-shot checks with a
/// proxy in the environment and watches where they land;
/// `wait_for_http_health` is covered by sharing this constructor.
pub(crate) fn loopback_client() -> reqwest::Result<Client> {
    gglib_proxy::loopback::client_builder()
        .timeout(Duration::from_secs(2))
        .build()
}

/// The shared health-check client, or an error if it could not be built.
///
/// Deliberately keeps the fallible signature rather than panicking: client
/// construction can fail on a misconfigured TLS backend, and a health check is
/// exactly the code path that should report that rather than abort the process.
fn health_client() -> Result<&'static Client> {
    if let Some(client) = HEALTH_CLIENT.get() {
        return Ok(client);
    }
    let client = loopback_client()?;
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

#[cfg(test)]
#[path = "health_proxy_tests.rs"]
mod health_proxy_tests;
