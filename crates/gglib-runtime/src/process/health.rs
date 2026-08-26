//! Health check utilities for llama-server processes.

use anyhow::Result;
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
/// This is a wall-clock bound, which it only became when the poll loop below
/// was given a real deadline. It used to be an *attempt count* — one second of
/// sleep plus a request that could itself take two — so a hung server that
/// accepted TCP and never answered stretched the same number to three times
/// its face value. The loosest case was exactly the one the ceiling exists to
/// bound.
const LAUNCH_DEADLINE_CEILING_SECS: u64 = 600;

/// Seconds of grace per GiB of weights.
const LAUNCH_DEADLINE_SECS_PER_GIB: u64 = 60;

/// How long to wait for a freshly spawned llama-server to answer `/health`,
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
/// NVMe and optimistic for a cold network filesystem, and the motivating case
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

/// Wait for HTTP health check to succeed
///
/// Polls the llama-server's /health endpoint until it returns 200 OK
/// or the timeout is reached.
pub async fn wait_for_http_health(port: u16, timeout_secs: u64) -> Result<()> {
    let health_url = format!("http://127.0.0.1:{}/health", port);
    info!("Waiting for llama-server to be ready at {}", health_url);

    // A wall-clock deadline, not an attempt count. Each pass costs a second of
    // sleep plus a request that can itself take two, so counting attempts made
    // `timeout_secs` mean anywhere between one and three times its face value
    // depending on whether the server refused the connection or accepted it
    // and hung — and the hung case, the one worth bounding, was the loosest.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
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

        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow::anyhow!(
                "llama-server failed to start within {}s on port {} (after {} probes). Check if the port is available.",
                timeout_secs,
                port,
                attempt
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
    /// Shared client, built once. See `crate::health::HEALTH_CLIENT` for why
    /// this isn't constructed per call — this path is hotter still, running on
    /// the already-running fast path of every proxied request.
    static CLIENT: std::sync::OnceLock<Option<reqwest::Client>> = std::sync::OnceLock::new();

    let health_url = format!("http://127.0.0.1:{port}/health");
    let Some(client) = CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .ok()
        })
        .as_ref()
    else {
        // Client construction failed — indistinguishable from an unhealthy
        // server as far as callers are concerned, matching this function's
        // "never returns an error" contract.
        return false;
    };

    matches!(
        client.get(&health_url).send().await,
        Ok(response) if response.status().is_success()
    )
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
