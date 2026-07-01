//! Background poller for llama.cpp's `GET /slots` endpoint.
//!
//! Kept deliberately separate from [`crate::slots`] (which is pure
//! fetch-and-parse): this module owns the *stateful* parts — the polling
//! interval, exponential backoff on failure, the "disabled" latch, and the
//! last-known-result cache — so `slots.rs` itself stays a small, easily
//! unit-tested leaf.
//!
//! ## Resilience
//!
//! [`fetch_slots`] never panics, and neither does this module: every branch
//! of the poll loop matches on a [`SlotsPollResult`] variant and either
//! updates the cache or adjusts the sleep duration. A struggling or
//! unreachable llama-server can only ever slow the poller down (via
//! [`next_backoff`], capped at [`MAX_POLL_BACKOFF`]) — it can never crash it
//! or block request-handling tasks, since it runs as its own `tokio::spawn`
//! task entirely isolated from the Axum handlers.
//!
//! ## Lifecycle
//!
//! [`spawn_slots_poller`] races its sleep against the shared shutdown
//! [`CancellationToken`] on every iteration, so it returns promptly when the
//! proxy server shuts down instead of sleeping out a long backoff first.
//! `serve()` awaits the returned `JoinHandle` after `axum::serve` completes,
//! so the task is always joined rather than left detached.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Client;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use gglib_core::ports::ModelRuntimePort;

use crate::slots::{SlotsPollResult, fetch_slots};

/// Polling cadence while llama-server is responding normally.
const BASE_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Ceiling for exponential backoff after consecutive failed polls.
const MAX_POLL_BACKOFF: Duration = Duration::from_secs(30);

// =============================================================================
// SlotsCache
// =============================================================================

/// Holds the most recent [`SlotsPollResult`], shared between the poller
/// task and (in a future phase) the dashboard HTTP handlers.
///
/// Uses `std::sync::Mutex`, following the same synchronous-critical-section
/// convention as [`crate::metrics::ContextMetricsStore`] and
/// [`crate::connections::ActiveConnectionsRegistry`]: `get`/`set` are a
/// single clone/assign with no `.await` inside, so the lock can never be
/// held across an await point.
pub struct SlotsCache {
    latest: Mutex<SlotsPollResult>,
}

impl Default for SlotsCache {
    fn default() -> Self {
        Self {
            latest: Mutex::new(SlotsPollResult::Unreachable(
                "no /slots poll has completed yet".to_string(),
            )),
        }
    }
}

impl SlotsCache {
    /// Create a cache with an initial "not polled yet" placeholder state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The most recently observed poll result.
    #[must_use]
    pub fn get(&self) -> SlotsPollResult {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Overwrite the cached result. Only called from the poller task.
    fn set(&self, result: SlotsPollResult) {
        *self.latest.lock().unwrap_or_else(|e| e.into_inner()) = result;
    }
}

// =============================================================================
// Backoff arithmetic (pure, unit-tested)
// =============================================================================

/// Compute the next backoff delay after a failed poll: double the current
/// delay, capped at [`MAX_POLL_BACKOFF`].
#[must_use]
fn next_backoff(current: Duration) -> Duration {
    current.saturating_mul(2).min(MAX_POLL_BACKOFF)
}

/// Decide the delay before the next poll attempt given the outcome of the
/// most recent one, or `None` to signal that polling should stop entirely
/// (the [`SlotsPollResult::Disabled`] case).
///
/// Pure and synchronous so the state-machine logic is unit-testable
/// without spinning up a real timer or task.
#[must_use]
fn next_delay(result: &SlotsPollResult, current_backoff: Duration) -> Option<Duration> {
    match result {
        SlotsPollResult::Available(_) => Some(BASE_POLL_INTERVAL),
        SlotsPollResult::Unreachable(_) => Some(next_backoff(current_backoff)),
        SlotsPollResult::Disabled => None,
    }
}

// =============================================================================
// Poller task
// =============================================================================

/// Spawn the background `/slots` poller as its own Tokio task.
///
/// Polls at [`BASE_POLL_INTERVAL`] while llama-server is reachable, with
/// exponential backoff (capped at [`MAX_POLL_BACKOFF`], reset to base on
/// the next success) while it is not. If `runtime_port.current_model()`
/// reports no model running, the HTTP call is skipped entirely for that
/// tick. If a `501`/`--no-slots` response is ever observed, the task logs
/// it once and returns for good — no further polling for the remainder of
/// this server run. In every case the task returns promptly once `cancel`
/// is triggered, rather than sleeping out a pending backoff first.
pub fn spawn_slots_poller(
    runtime_port: Arc<dyn ModelRuntimePort>,
    client: Client,
    cache: Arc<SlotsCache>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut backoff = BASE_POLL_INTERVAL;

        loop {
            let sleep_for = match runtime_port.current_model().await {
                None => BASE_POLL_INTERVAL,
                Some(target) => {
                    let result = fetch_slots(&client, &target.base_url).await;
                    if let SlotsPollResult::Unreachable(ref msg) = result {
                        warn!(
                            "proxy dashboard: /slots poll failed ({msg}); backing off to {backoff:?}"
                        );
                    }
                    let delay = next_delay(&result, backoff);
                    cache.set(result);
                    match delay {
                        Some(delay) => {
                            backoff = delay;
                            delay
                        }
                        None => {
                            info!(
                                "proxy dashboard: /slots endpoint is disabled upstream (--no-slots); poller stopping"
                            );
                            return;
                        }
                    }
                }
            };

            tokio::select! {
                () = cancel.cancelled() => {
                    debug!("proxy dashboard: /slots poller shutting down");
                    return;
                }
                () = tokio::time::sleep(sleep_for) => {}
            }
        }
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use gglib_core::ports::{ModelRuntimeError, RunningTarget};

    #[test]
    fn next_backoff_doubles() {
        assert_eq!(next_backoff(Duration::from_secs(1)), Duration::from_secs(2));
        assert_eq!(next_backoff(Duration::from_secs(2)), Duration::from_secs(4));
    }

    #[test]
    fn next_backoff_caps_at_ceiling() {
        assert_eq!(
            next_backoff(Duration::from_secs(20)),
            Duration::from_secs(30)
        );
        assert_eq!(
            next_backoff(Duration::from_secs(30)),
            Duration::from_secs(30)
        );
        assert_eq!(
            next_backoff(Duration::from_secs(100)),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn next_delay_resets_to_base_on_success() {
        let result = SlotsPollResult::Available(vec![]);
        assert_eq!(
            next_delay(&result, Duration::from_secs(16)),
            Some(BASE_POLL_INTERVAL)
        );
    }

    #[test]
    fn next_delay_backs_off_on_unreachable() {
        let result = SlotsPollResult::Unreachable("connection refused".to_string());
        assert_eq!(
            next_delay(&result, Duration::from_secs(4)),
            Some(Duration::from_secs(8))
        );
    }

    #[test]
    fn next_delay_signals_stop_on_disabled() {
        assert_eq!(
            next_delay(&SlotsPollResult::Disabled, Duration::from_secs(1)),
            None
        );
    }

    #[test]
    fn cache_defaults_to_unreachable_placeholder() {
        let cache = SlotsCache::new();
        assert!(matches!(cache.get(), SlotsPollResult::Unreachable(_)));
    }

    #[test]
    fn cache_get_reflects_latest_set() {
        let cache = SlotsCache::new();
        cache.set(SlotsPollResult::Disabled);
        assert_eq!(cache.get(), SlotsPollResult::Disabled);
    }

    /// A `ModelRuntimePort` that always reports no model running, so the
    /// poller never makes an HTTP call and there is nothing to mock.
    #[derive(Debug)]
    struct NoModelRunning;

    #[async_trait]
    impl ModelRuntimePort for NoModelRunning {
        async fn ensure_model_running(
            &self,
            model: &str,
            _num_ctx: Option<u64>,
            _default_ctx: u64,
        ) -> Result<RunningTarget, ModelRuntimeError> {
            Err(ModelRuntimeError::ModelNotFound(model.to_string()))
        }
        async fn current_model(&self) -> Option<RunningTarget> {
            None
        }
        async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn poller_shuts_down_promptly_on_cancellation() {
        let cancel = CancellationToken::new();
        let cache = Arc::new(SlotsCache::new());
        let handle = spawn_slots_poller(
            Arc::new(NoModelRunning),
            Client::new(),
            cache,
            cancel.clone(),
        );

        cancel.cancel();

        // The poller must return promptly rather than leaking or hanging;
        // give it a generous but bounded window well under the base poll
        // interval so a regression (e.g. missing cancellation check) fails
        // fast instead of hanging the test suite.
        tokio::time::timeout(Duration::from_millis(500), handle)
            .await
            .expect("poller task did not shut down promptly after cancellation")
            .expect("poller task panicked");
    }
}
