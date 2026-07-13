//! Concurrent startup guard using tokio::sync::watch channels.
//!
//! Replaces the `AtomicBool` loading flag with a watch-channel-based mechanism
//! so that concurrent requests during model startup WAIT for the result instead
//! of immediately failing with a generic "model_loading" error.

use std::sync::{Arc, RwLock};

use gglib_core::ports::{ModelRuntimeError, RunningTarget};
use tokio::sync::watch;
use tokio::time::{Duration, timeout};

/// Default timeout for waiters awaiting model startup (120s health check + 30s margin).
pub const STARTUP_WAIT_TIMEOUT: Duration = Duration::from_secs(150);

/// Minimum time a waiter must have remaining before attempting its own startup.
/// Derived from STARTUP_WAIT_TIMEOUT — ensures the waiter has enough budget for
/// model resolution + spawn + health check after bouncing off other startups.
pub const MIN_STARTUP_BUDGET: Duration = Duration::from_secs(75);

/// Check whether the remaining time budget is too small to attempt startup.
///
/// Returns `true` when `remaining < MIN_STARTUP_BUDGET`, indicating that a
/// cross-model waiter should bail with [`ModelRuntimeError::ContentionTimeout`
/// ] instead of attempting a startup that would almost certainly time out.
pub fn should_bail_on_insufficient_budget(remaining: Duration) -> bool {
    remaining < MIN_STARTUP_BUDGET
}

/// Type alias for the shared loading slot.
pub type LoadingSlot = Arc<RwLock<Option<StartupState>>>;

/// Holds the watch channel receiver for an in-progress model startup.
/// Stored inside `ProcessStrategy::SingleSwap.loading` (behind `Arc<RwLock>`).
pub struct StartupState {
    receiver: watch::Receiver<Result<RunningTarget, ModelRuntimeError>>,
    /// Which model this startup is targeting (set by the initiator).
    target_model_name: String,
}

impl StartupState {
    /// Create a new startup state, guard, and an initiator receiver.
    ///
    /// - `state` is stored in the loading slot.
    /// - `guard` is held by the spawned driver task.
    /// - `initiator_rx` is for the caller that triggered the start (waits like everyone else).
    pub fn new(
        slot: LoadingSlot,
        target_model_name: String,
    ) -> (
        Self,
        ModelStartupGuard,
        watch::Receiver<Result<RunningTarget, ModelRuntimeError>>,
    ) {
        let initial = Err(ModelRuntimeError::ModelLoading);
        let (tx, rx) = watch::channel(initial);
        // Subscribe BEFORE inserting into slot — initiator sees all sends.
        let initiator_rx = tx.subscribe();
        let state = Self {
            receiver: rx,
            target_model_name,
        };
        let guard = ModelStartupGuard {
            sender: Some(tx),
            slot,
        };
        (state, guard, initiator_rx)
    }

    /// Clone a receiver for a waiter to subscribe.
    pub fn subscribe(&self) -> watch::Receiver<Result<RunningTarget, ModelRuntimeError>> {
        self.receiver.clone()
    }

    /// Which model this startup is targeting.
    pub fn target_model_name(&self) -> &str {
        &self.target_model_name
    }
}

/// Result of the atomic check-and-insert on the loading slot.
pub enum StartupDisposition {
    /// Another driver is active — wait for its result.
    /// The `target_model_name` tells you which model this startup is for.
    Waiter {
        rx: watch::Receiver<Result<RunningTarget, ModelRuntimeError>>,
        target_model_name: String,
    },
    /// We claimed the slot — spawn the driver task and wait via our own receiver.
    Initiator {
        guard: ModelStartupGuard,
        self_rx: watch::Receiver<Result<RunningTarget, ModelRuntimeError>>,
    },
}

impl StartupDisposition {
    /// Perform the atomic check-and-insert on the loading slot.
    ///
    /// Returns `Waiter` if a driver is already active (subscribes to its channel).
    /// Returns `Initiator` if we claimed the slot (includes guard + our own receiver).
    pub fn check(slot: &LoadingSlot, target_model_name: String) -> Self {
        let mut s = slot.write().unwrap_or_else(|p| p.into_inner());
        match &*s {
            Some(state) => StartupDisposition::Waiter {
                rx: state.subscribe(),
                target_model_name: state.target_model_name().to_string(),
            },
            None => {
                let (state, guard, self_rx) = StartupState::new(slot.clone(), target_model_name);
                *s = Some(state);
                StartupDisposition::Initiator { guard, self_rx }
            }
        }
    }
}

/// Single consolidated RAII guard held by the spawned driver task.
///
/// - `succeed()` / `fail()` — sends Result through channel, then clears the loading slot.
/// - `drop()` (panic/cancellation) — sends Internal error, then clears the slot.
///
/// Order is fixed: notify first, then clear. A new driver cannot start until the slot
/// is cleared, so waiters always receive a result before any replacement startup begins.
pub struct ModelStartupGuard {
    sender: Option<watch::Sender<Result<RunningTarget, ModelRuntimeError>>>,
    slot: LoadingSlot,
}

impl ModelStartupGuard {
    /// Send success result, clear the slot. Consumes sender so drop is a no-op.
    pub fn succeed(mut self, target: RunningTarget) -> Result<RunningTarget, ModelRuntimeError> {
        if let Some(tx) = self.sender.take() {
            let _ = tx.send(Ok(target.clone()));
        }
        // Clear the slot so future calls can start fresh.
        let mut slot = self.slot.write().unwrap_or_else(|p| p.into_inner());
        *slot = None;
        Ok(target)
    }

    /// Send error result, clear the slot. Consumes sender so drop is a no-op.
    pub fn fail(mut self, err: ModelRuntimeError) -> Result<RunningTarget, ModelRuntimeError> {
        if let Some(tx) = self.sender.take() {
            let _ = tx.send(Err(err.clone()));
        }
        let mut slot = self.slot.write().unwrap_or_else(|p| p.into_inner());
        *slot = None;
        Err(err)
    }
}

impl Drop for ModelStartupGuard {
    fn drop(&mut self) {
        // Only clear the slot if sender was not taken (panic/cancellation path).
        // If succeed()/fail() already ran, another driver may own the slot now — don't wipe it.
        if let Some(tx) = self.sender.take() {
            let _ = tx.send(Err(ModelRuntimeError::Internal(
                "Startup driver panicked or was dropped unexpectedly".to_string(),
            )));
            let mut slot = self.slot.write().unwrap_or_else(|p| p.into_inner());
            *slot = None;
        }
    }
}

/// Spawn the actual startup work in a detached task, wiring the guard for notification.
///
/// The spawned task runs `work` bounded by `driver_timeout`. On success it calls
/// `guard.succeed()`, on error `guard.fail()`. If the driver exceeds its deadline,
/// it self-terminates and broadcasts an Internal "Driver exceeded startup deadline" error.
/// If the task panics, `guard::drop()` sends an Internal error.
///
/// The caller (initiator or waiter) should then `wait_for_startup()` on its own receiver.
pub fn drive<F>(guard: ModelStartupGuard, driver_timeout: Duration, work: F)
where
    F: std::future::Future<Output = Result<RunningTarget, ModelRuntimeError>> + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(async move {
        match tokio::time::timeout(driver_timeout, work).await {
            Ok(Ok(target)) => {
                let _ = guard.succeed(target);
            }
            Ok(Err(err)) => {
                let _ = guard.fail(err);
            }
            Err(_) => {
                let _ = guard.fail(ModelRuntimeError::Internal(
                    "Driver exceeded startup deadline".to_string(),
                ));
            }
        }
    });
}

/// Wait for the startup result with a timeout.
pub async fn wait_for_startup(
    mut rx: watch::Receiver<Result<RunningTarget, ModelRuntimeError>>,
    timeout_duration: Duration,
) -> Result<RunningTarget, ModelRuntimeError> {
    match timeout(timeout_duration, rx.changed()).await {
        Ok(Ok(())) => {
            // Channel updated — retrieve the result.
            rx.borrow_and_update().clone()
        }
        Ok(Err(_)) => {
            // Channel closed — retrieve the last value (preserves specific errors).
            rx.borrow_and_update().clone()
        }
        Err(_) => Err(ModelRuntimeError::Internal(
            "Startup timed out — driver may be stuck".to_string(),
        )),
    }
}

#[cfg(test)]
#[path = "startup_guard_tests.rs"]
mod tests;
