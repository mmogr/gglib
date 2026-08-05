//! Bounded wait for model-startup contention.

use std::sync::Arc;
use std::time::Duration;

use gglib_core::ports::{ModelRuntimeError, ModelRuntimePort, RunningTarget};
use gglib_core::retry::{RetryDecision, RetryPolicy, decide, jitter_unit};
// Tokio's clock rather than `std`'s: identical in a normal runtime, but it is
// the one `tokio::time::pause()` advances, which is what lets the tests below
// drive a whole wait window without sleeping through it.
use tokio::time::Instant;
use tracing::{debug, warn};

/// Environment override for the contention window, in seconds.
pub const WAIT_ENV_VAR: &str = "GGLIB_CONTENTION_WAIT_SECS";

/// How long the proxy absorbs contention before surfacing a 503.
///
/// Long enough for a competing model's startup to finish and release the slot,
/// short enough that a client which has given up waiting has not been holding a
/// connection for minutes.
pub const DEFAULT_WAIT: Duration = Duration::from_secs(30);

/// Resolve the contention window from the environment, once.
///
/// An unset or unparseable value falls back to [`DEFAULT_WAIT`]. Zero is
/// meaningful and preserved: it restores fail-fast, surfacing the 503 to the
/// client immediately.
#[must_use]
pub fn wait_from_env() -> Duration {
    match std::env::var(WAIT_ENV_VAR) {
        Ok(raw) => raw.trim().parse::<u64>().map_or_else(
            |_| {
                warn!(
                    value = %raw,
                    "{WAIT_ENV_VAR} is not a whole number of seconds — using the default"
                );
                DEFAULT_WAIT
            },
            Duration::from_secs,
        ),
        Err(_) => DEFAULT_WAIT,
    }
}

/// Backoff schedule for polling a contended slot.
///
/// The wall-clock `window` is the only real bound — there is no separate
/// attempt budget, because how many polls fit inside it depends entirely on how
/// quickly the runtime returns. Short delays keep the request responsive when
/// the slot frees early; the 2 s ceiling stops a long window becoming a busy
/// loop.
fn polling_policy(window: Duration) -> RetryPolicy {
    RetryPolicy {
        max_attempts: u32::MAX,
        initial_backoff: Duration::from_millis(250),
        max_backoff: Duration::from_secs(2),
        total_deadline: window,
    }
}

/// Ensure `model_name` is running, absorbing startup contention for up to
/// `window` before giving up.
///
/// Only [`ModelRuntimeError::ContentionTimeout`] is waited on. Every other
/// error — including [`ModelRuntimeError::ModelLoading`], which the caller
/// already retries on its own longer schedule — is returned untouched, so this
/// changes nothing outside the contention path.
///
/// A `window` of zero returns the first contention error immediately, which is
/// the fail-fast behaviour this function replaced.
///
/// # Errors
///
/// The last error the runtime produced. When the window elapses while still
/// contended, that is a `ContentionTimeout` the caller turns into a 503.
pub async fn ensure_with_contention_wait(
    runtime: &Arc<dyn ModelRuntimePort>,
    model_name: &str,
    num_ctx: Option<u64>,
    default_ctx: u64,
    window: Duration,
) -> Result<RunningTarget, ModelRuntimeError> {
    ensure_with(
        runtime,
        model_name,
        num_ctx,
        default_ctx,
        window,
        jitter_unit,
    )
    .await
}

/// [`ensure_with_contention_wait`], with the randomness supplied by the caller.
///
/// The same seam [`decide`] and
/// [`RetryPolicy::with_env_overrides`](gglib_core::retry::RetryPolicy) already
/// have: the impurity is a parameter, so a test states the draw it wants and
/// asserts the resulting delays exactly instead of hoping the dice fall its way.
/// Production passes [`jitter_unit`] and behaves as before.
///
/// Full jitter can return a delay of zero, so a caller that pins `jitter` at
/// `0.0` against a permanently contended runtime will never advance `elapsed`
/// and never reach the deadline — `polling_policy` sets no attempt ceiling on
/// purpose. Real jitter makes that a measure-zero event; a fixed draw makes it
/// reachable, so tests use a non-zero one.
async fn ensure_with<J: Fn() -> f64>(
    runtime: &Arc<dyn ModelRuntimePort>,
    model_name: &str,
    num_ctx: Option<u64>,
    default_ctx: u64,
    window: Duration,
    jitter: J,
) -> Result<RunningTarget, ModelRuntimeError> {
    let started = Instant::now();
    let policy = polling_policy(window);
    let mut attempt: u32 = 0;

    loop {
        attempt += 1;
        let contention = match runtime
            .ensure_model_running(model_name, num_ctx, default_ctx)
            .await
        {
            Ok(target) => return Ok(target),
            Err(ModelRuntimeError::ContentionTimeout(msg)) => msg,
            Err(other) => return Err(other),
        };

        if window.is_zero() {
            return Err(ModelRuntimeError::ContentionTimeout(contention));
        }

        let elapsed = started.elapsed();
        match decide(&policy, attempt, None, elapsed, jitter()) {
            RetryDecision::Retry { after } => {
                debug!(
                    attempt,
                    delay_ms = after.as_millis(),
                    model = model_name,
                    "startup contention — waiting before re-checking"
                );
                tokio::time::sleep(after).await;
            }
            RetryDecision::GiveUp(reason) => {
                warn!(
                    attempts = attempt,
                    elapsed_ms = elapsed.as_millis(),
                    model = model_name,
                    give_up = reason.as_str(),
                    "startup contention outlasted the wait window — surfacing 503"
                );
                return Err(ModelRuntimeError::ContentionTimeout(contention));
            }
        }
    }
}

#[cfg(test)]
#[path = "wait_tests.rs"]
mod wait_tests;
