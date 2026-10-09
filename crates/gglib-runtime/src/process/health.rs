//! Health checks for the servers gglib starts, asked the way each runtime
//! answers: llama-server at `/health`, `sd-server` at `/v1/models` (see
//! [`RuntimeKind::health_path`]).

use anyhow::Result;
use gglib_core::domain::RuntimeKind;
use tokio::time::{Duration, sleep};
use tracing::{debug, info};

/// Lower bound on a launch deadline, and the value used when the model's size
/// is unknown.
const LAUNCH_DEADLINE_FLOOR_SECS: u64 = 120;

/// Upper bound on a launch deadline.
///
/// A user-experience limit, not a technical one: past this point a server that
/// is not coming up is holding a slot while somebody watches a spinner, and
/// surfacing the error beats waiting longer.
///
/// A wall-clock bound: [`wait_for_http_health`] polls against a deadline, not
/// an attempt count.
const LAUNCH_DEADLINE_CEILING_SECS: u64 = 600;

/// Seconds of grace per GiB of weights.
const LAUNCH_DEADLINE_SECS_PER_GIB: u64 = 60;

/// How long to wait for a freshly spawned server to answer its health probe,
/// scaled to how much it has to load.
///
/// A flat timeout is wrong in both directions. Too short and a large model on
/// a first run — where the weights are cold and, on Apple hardware, Metal is
/// compiling its shader pipeline — is killed while it was still working. Too
/// long and a server that will never answer occupies a slot, and the person
/// waiting sees a spinner rather than an error.
///
/// `weights_bytes == 0` means the size is unknown, which yields the floor
/// rather than an optimistic guess.
///
/// **The per-GiB constant is a guess about the host, not a fact about the
/// model.** 60s/GiB is roughly 17 MiB/s effective, which is pessimistic for
/// `NVMe` and optimistic for a cold network filesystem, and the motivating case
/// — a first-run Metal shader compile — scales with the kernel set rather than
/// with file size at all. It is a bounded, deliberately generous budget that
/// buys a large model its first load; it is not a model of anything. The
/// principled version observes progress (llama-server answers `/health` while
/// loading, and its stderr is already being read) and resets a much shorter
/// deadline whenever the load advances, which would need no constant and would
/// catch a genuinely hung server sooner. That is the better design and it is
/// not this one.
#[must_use]
pub(crate) const fn launch_deadline_secs(weights_bytes: u64) -> u64 {
    const GIB: u64 = 1024 * 1024 * 1024;
    // Round up: a 4.2 GiB model should be budgeted as 5, not 4.
    let gib = weights_bytes.div_ceil(GIB);
    let scaled = gib.saturating_mul(LAUNCH_DEADLINE_SECS_PER_GIB);

    if scaled < LAUNCH_DEADLINE_FLOOR_SECS {
        LAUNCH_DEADLINE_FLOOR_SECS
    } else if scaled > LAUNCH_DEADLINE_CEILING_SECS {
        LAUNCH_DEADLINE_CEILING_SECS
    } else {
        scaled
    }
}

/// Wait for a freshly spawned server to answer its health probe.
///
/// Polls `runtime`'s [`health_path`](RuntimeKind::health_path) until it
/// answers 200 with a body [`is_ready_body`](RuntimeKind::is_ready_body)
/// accepts, or the timeout is reached. A 200 that is not this runtime's
/// server fails after a few tries rather than at the deadline: something
/// else holds the port.
pub async fn wait_for_http_health(
    port: u16,
    timeout_secs: u64,
    runtime: RuntimeKind,
) -> Result<()> {
    let server = runtime.server_name();
    let health_url = format!("http://127.0.0.1:{port}{}", runtime.health_path());
    info!("Waiting for {server} to be ready at {health_url}");

    // A wall-clock deadline, not an attempt count. Each pass costs a second of
    // sleep plus a request that can itself take two, so counting attempts
    // would make `timeout_secs` mean anywhere between one and three times its
    // face value depending on whether the server refused the connection or
    // accepted it and hung — loosest in the hung case, the one worth bounding.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    let mut attempt = 0;
    let client = crate::health::loopback_client()?;

    loop {
        attempt += 1;
        sleep(Duration::from_secs(1)).await;

        match client.get(&health_url).send().await {
            Ok(response) => {
                let status = response.status();

                // Only accept 200 OK - anything else is wrong
                if status.is_success() {
                    // Got 200 OK - verify it is this runtime's server
                    match response.text().await {
                        Ok(body) => {
                            if runtime.is_ready_body(&body) {
                                info!("{server} is ready on port {port}");
                                return Ok(());
                            }
                            debug!("Health check returned unexpected response: {}", body);
                            if attempt > 5 {
                                return Err(anyhow::anyhow!(
                                    "Port {port} is responding but doesn't appear to be {server}"
                                ));
                            }
                        }
                        Err(e) => {
                            debug!("Failed to read health response: {}", e);
                        }
                    }
                } else {
                    debug!(
                        "Health check returned status {} (expected 200), retrying...",
                        status
                    );

                    // If we get a clear error from wrong service, fail faster
                    if (status.as_u16() == 403 || status.as_u16() == 404) && attempt > 3 {
                        return Err(anyhow::anyhow!(
                            "Port {port} appears to be in use by another service (status {status}). Try using a different port range."
                        ));
                    }
                }
            }
            Err(e) => {
                debug!("Health check failed: {}, retrying...", e);
            }
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow::anyhow!(
                "{server} failed to start within {timeout_secs}s on port {port} (after {attempt} probes). Check if the port is available."
            ));
        }
    }
}

/// Single-shot HTTP health probe of a running server, asked the way
/// `runtime` answers.
///
/// Unlike [`wait_for_http_health`] this does **not** retry: it makes one
/// request with a short timeout and reports whether the server responded
/// with a 2xx. The "already running" fast path uses it to detect a cached
/// server that has silently degraded or wedged, so the caller can recycle it
/// instead of routing a request into a dead instance, and
/// [`ServerHealthChecker`](crate::health_monitor::ServerHealthChecker) polls
/// with it.
///
/// llama-server is judged by status alone, as it always was. `sd-server` is
/// asked at `/v1/models` and its body must name `sd-cpp-local`: that route
/// answers while a render holds the server's lock, so a drawing server reads
/// as healthy, and a different server that took the port does not.
///
/// Never returns an error — any failure (connection refused, timeout,
/// non-2xx, the wrong server) is reported as `false` so callers can treat
/// "not healthy" and "unreachable" identically.
pub async fn check_http_health(port: u16, runtime: RuntimeKind) -> bool {
    /// Shared client, built once: this runs on the already-running fast path
    /// of every proxied request and whenever the health monitor polls a live
    /// process, and a client per call would initialize a TLS backend and
    /// throw the connection pool away each time.
    /// `crate::health::loopback_client` says why it never consults a proxy.
    static CLIENT: std::sync::OnceLock<Option<reqwest::Client>> = std::sync::OnceLock::new();

    let health_url = format!("http://127.0.0.1:{port}{}", runtime.health_path());
    let Some(client) = CLIENT
        .get_or_init(|| crate::health::loopback_client().ok())
        .as_ref()
    else {
        // Client construction failed — indistinguishable from an unhealthy
        // server as far as callers are concerned, matching this function's
        // "never returns an error" contract.
        return false;
    };

    let Ok(response) = client.get(&health_url).send().await else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    match runtime {
        RuntimeKind::Llama => true,
        RuntimeKind::StableDiffusion => response
            .text()
            .await
            .is_ok_and(|body| runtime.is_ready_body(&body)),
    }
}

#[cfg(test)]
mod tests {
    use super::{LAUNCH_DEADLINE_CEILING_SECS, LAUNCH_DEADLINE_FLOOR_SECS, launch_deadline_secs};

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn launch_deadline_scales_with_weight_and_is_bounded() {
        // Small models sit on the floor: 1 GiB would score 60s, which is less
        // grace than a cold start needs even for something tiny.
        assert_eq!(launch_deadline_secs(GIB), LAUNCH_DEADLINE_FLOOR_SECS);
        assert_eq!(launch_deadline_secs(2 * GIB), LAUNCH_DEADLINE_FLOOR_SECS);

        // The scaling band.
        assert_eq!(launch_deadline_secs(4 * GIB), 240);
        assert_eq!(launch_deadline_secs(8 * GIB), 480);

        // And the ceiling holds however large the model is.
        assert_eq!(launch_deadline_secs(16 * GIB), LAUNCH_DEADLINE_CEILING_SECS);
        assert_eq!(launch_deadline_secs(70 * GIB), LAUNCH_DEADLINE_CEILING_SECS);
        assert_eq!(launch_deadline_secs(u64::MAX), LAUNCH_DEADLINE_CEILING_SECS);
    }

    #[test]
    fn a_partial_gibibyte_rounds_up() {
        // 4.2 GiB is budgeted as 5, not 4: rounding down would shave a minute
        // off exactly the models nearest the floor.
        assert_eq!(launch_deadline_secs(4 * GIB + 1), 300);
    }

    #[test]
    fn an_unknown_size_gets_the_floor_not_an_optimistic_guess() {
        assert_eq!(launch_deadline_secs(0), LAUNCH_DEADLINE_FLOOR_SECS);
    }
}
