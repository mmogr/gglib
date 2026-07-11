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
/// The spawned task runs `work`. On success it calls `guard.succeed()`, on error
/// `guard.fail()`. If the task panics, `guard::drop()` sends an Internal error.
///
/// The caller (initiator or waiter) should then `wait_for_startup()` on its own receiver.
pub fn drive<F>(guard: ModelStartupGuard, work: F)
where
    F: std::future::Future<Output = Result<RunningTarget, ModelRuntimeError>> + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(async move {
        let result = work.await;
        match result {
            Ok(target) => {
                let _ = guard.succeed(target);
            }
            Err(err) => {
                let _ = guard.fail(err);
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
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    /// Test 1: Concurrent callers both succeed (the VS Code scenario).
    #[tokio::test]
    async fn test_concurrent_callers_both_succeed() {
        let slot = Arc::new(RwLock::new(None));
        let target = RunningTarget::local(8080, 1, "test-model".to_string(), 2048);

        // Clone target for each async block (they both move into the closure)
        let target1 = target.clone();
        let target2 = target.clone();

        // Spawn two concurrent callers
        let (handle1, handle2) = tokio::join!(
            async {
                let disp = StartupDisposition::check(&slot, "test-model".to_string());
                match disp {
                    StartupDisposition::Waiter { rx, .. } => {
                        wait_for_startup(rx, Duration::from_secs(5)).await
                    }
                    StartupDisposition::Initiator { guard, self_rx } => {
                        // Simulate startup work: short delay then succeed
                        drive(guard, async move {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            Ok(target1.clone())
                        });
                        wait_for_startup(self_rx, Duration::from_secs(5)).await
                    }
                }
            },
            async {
                // Small delay to ensure first caller has already claimed the slot
                tokio::time::sleep(Duration::from_millis(10)).await;
                let disp = StartupDisposition::check(&slot, "test-model".to_string());
                match disp {
                    StartupDisposition::Waiter { rx, .. } => {
                        wait_for_startup(rx, Duration::from_secs(5)).await
                    }
                    StartupDisposition::Initiator { guard, self_rx } => {
                        drive(guard, async move {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            Ok(target2.clone())
                        });
                        wait_for_startup(self_rx, Duration::from_secs(5)).await
                    }
                }
            }
        );

        // Both should succeed with the same target
        assert!(handle1.is_ok(), "Caller 1 should succeed: {:?}", handle1);
        assert!(handle2.is_ok(), "Caller 2 should succeed: {:?}", handle2);
        assert_eq!(handle1.unwrap().model_name, "test-model");
        assert_eq!(handle2.unwrap().model_name, "test-model");

        // Give detached task time to finish clearing the slot (send happens before clear,
        // so wait_for_startup returns while the task is still executing slot-clear code)
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Slot should be cleared after success
        assert!(
            slot.read().unwrap().is_none(),
            "Slot should be cleared after startup completes"
        );
    }

    /// Test 2: Waiters receive specific error on failure (not generic "model_loading").
    #[tokio::test]
    async fn test_waiters_get_specific_error_on_failure() {
        let slot = Arc::new(RwLock::new(None));
        let expected_err = ModelRuntimeError::SpawnFailed("OOM killed".to_string());

        // Driver fails with specific error (delayed so waiter can arrive first)
        let driver_handle = tokio::spawn({
            let slot_clone = slot.clone();
            async move {
                let disp = StartupDisposition::check(&slot_clone, "test-model".to_string());
                match disp {
                    StartupDisposition::Initiator { guard, self_rx } => {
                        drive(guard, async move {
                            // Delay so waiter has time to subscribe before we fail
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            Err(expected_err.clone())
                        });
                        wait_for_startup(self_rx, Duration::from_secs(5)).await
                    }
                    _ => unreachable!("Should be initiator"),
                }
            }
        });

        // Waiter arrives while driver is still running (before the 200ms delay)
        tokio::time::sleep(Duration::from_millis(50)).await;
        let waiter_result = {
            let disp = StartupDisposition::check(&slot, "test-model".to_string());
            match disp {
                StartupDisposition::Waiter { rx, .. } => {
                    wait_for_startup(rx, Duration::from_secs(5)).await
                }
                _ => unreachable!("Should be waiter"),
            }
        };

        // Both should get the SPECIFIC error (SpawnFailed), not ModelLoading or Internal
        let driver_err = driver_handle.await.unwrap().unwrap_err();
        assert!(
            matches!(&driver_err, ModelRuntimeError::SpawnFailed(msg) if msg == "OOM killed"),
            "Driver should get specific SpawnFailed error, got: {:?}",
            driver_err
        );

        let waiter_err = waiter_result.unwrap_err();
        assert!(
            matches!(&waiter_err, ModelRuntimeError::SpawnFailed(msg) if msg == "OOM killed"),
            "Waiter should get specific SpawnFailed error, got: {:?}",
            waiter_err
        );
    }

    /// Test 3: Cancellation safety — dropping the initiator's future doesn't affect waiters.
    #[tokio::test]
    async fn test_initiator_cancellation_does_not_affect_waiters() {
        let slot = Arc::new(RwLock::new(None));
        let target = RunningTarget::local(8080, 1, "test-model".to_string(), 2048);

        // Initiator spawns the driver task but then gets cancelled (future dropped)
        let initiator_handle = tokio::spawn({
            let slot_clone = slot.clone();
            async move {
                let disp = StartupDisposition::check(&slot_clone, "test-model".to_string());
                match disp {
                    StartupDisposition::Initiator { guard, self_rx: _ } => {
                        drive(guard, async move {
                            // Simulate a longer startup
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            Ok(target.clone())
                        });
                        // NOTE: We do NOT await wait_for_startup here — the initiator's
                        // future is dropped immediately, simulating client disconnect.
                        // The driver task continues running in the background.
                    }
                    _ => unreachable!(),
                }
            }
        });

        // Wait for initiator to complete (it drops without waiting)
        let _ = initiator_handle.await;

        // A waiter should still be able to subscribe and get the result
        tokio::time::sleep(Duration::from_millis(50)).await;
        let waiter_result = {
            let disp = StartupDisposition::check(&slot, "test-model".to_string());
            match disp {
                StartupDisposition::Waiter { rx, .. } => {
                    wait_for_startup(rx, Duration::from_secs(5)).await
                }
                _ => unreachable!("Should be waiter — driver is still running"),
            }
        };

        // Waiter should succeed despite initiator being cancelled
        assert!(
            waiter_result.is_ok(),
            "Waiter should succeed: {:?}",
            waiter_result
        );
        assert_eq!(waiter_result.unwrap().model_name, "test-model");

        // Give detached task time to finish clearing the slot
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    /// Test 4: Guard drop (panic path) sends Internal error and clears slot.
    #[tokio::test]
    async fn test_guard_drop_clears_slot_and_notifies_waiters() {
        let slot = Arc::new(RwLock::new(None));

        // Create a guard but drop it without calling succeed/fail (simulates panic)
        {
            let disp = StartupDisposition::check(&slot, "test-model".to_string());
            match disp {
                StartupDisposition::Initiator { guard, .. } => {
                    // guard is dropped at end of this block without succeed/fail
                    drop(guard);
                }
                _ => unreachable!(),
            }
        }

        // Slot should be cleared (Drop runs synchronously)
        let s = slot.read().unwrap();
        assert!(s.is_none(), "Slot should be cleared after guard drop");
    }

    /// Test 5: Sequential startups — no stale state between cycles.
    #[tokio::test]
    async fn test_sequential_startups_clean_up_properly() {
        let slot = Arc::new(RwLock::new(None));

        // First startup cycle
        let target1 = RunningTarget::local(8080, 1, "model-v1".to_string(), 2048);
        let result1 = {
            let disp = StartupDisposition::check(&slot, "model-v1".to_string());
            match disp {
                StartupDisposition::Initiator { guard, self_rx } => {
                    drive(guard, async move {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        Ok(target1.clone())
                    });
                    wait_for_startup(self_rx, Duration::from_secs(5)).await
                }
                _ => unreachable!(),
            }
        };
        assert!(result1.is_ok());

        // Give detached task time to finish clearing the slot
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Slot should be cleared between cycles (scoped read guard to avoid deadlock)
        {
            assert!(
                slot.read().unwrap().is_none(),
                "Slot should be None between startup cycles"
            );
        }

        // Second startup cycle — should get Initiator again (not Waiter on stale channel)
        let target2 = RunningTarget::local(8081, 2, "model-v2".to_string(), 4096);
        let result2 = {
            let disp = StartupDisposition::check(&slot, "model-v2".to_string());
            match disp {
                StartupDisposition::Initiator { guard, self_rx } => {
                    drive(guard, async move {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        Ok(target2.clone())
                    });
                    wait_for_startup(self_rx, Duration::from_secs(5)).await
                }
                _ => panic!("Should be Initiator on fresh slot, not Waiter"),
            }
        };
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap().model_name, "model-v2");
    }

    /// Test 6: Waiter timeout fires when driver is stuck.
    #[tokio::test]
    async fn test_waiter_timeout_fires_on_stuck_driver() {
        let slot = Arc::new(RwLock::new(None));

        // Driver that never completes (simulates hung health check)
        {
            let disp = StartupDisposition::check(&slot, "stuck".to_string());
            match disp {
                StartupDisposition::Initiator { guard, self_rx: _ } => {
                    drive(guard, async move {
                        // Never returns — simulates stuck driver
                        tokio::time::sleep(Duration::from_secs(999)).await;
                        Ok(RunningTarget::local(8080, 1, "stuck".to_string(), 2048))
                    });
                    // Don't await self_rx — just let the driver hang
                }
                _ => unreachable!(),
            }
        }

        // Waiter with short timeout should get Internal("timed out")
        tokio::time::sleep(Duration::from_millis(50)).await;
        let result = {
            let disp = StartupDisposition::check(&slot, "stuck".to_string());
            match disp {
                StartupDisposition::Waiter { rx, .. } => {
                    wait_for_startup(rx, Duration::from_millis(200)).await
                }
                _ => unreachable!(),
            }
        };

        assert!(
            matches!(&result, Err(ModelRuntimeError::Internal(msg)) if msg.contains("timed out")),
            "Should get timeout error, got: {:?}",
            result
        );
    }

    /// Test 7: Cross-model race condition — concurrent callers for different models each get correct result.
    #[tokio::test]
    async fn test_concurrent_different_models_each_get_correct_result() {
        let slot = Arc::new(RwLock::new(None));

        let target_a = RunningTarget::local(8080, 1, "model-a".to_string(), 2048);
        let target_b = RunningTarget::local(8081, 2, "model-b".to_string(), 4096);

        // Two concurrent callers for DIFFERENT models
        let (result_a, result_b) = tokio::join!(
            async {
                loop {
                    let disp = StartupDisposition::check(&slot, "model-a".to_string());
                    match disp {
                        StartupDisposition::Waiter {
                            rx,
                            target_model_name,
                        } => {
                            if target_model_name == "model-a" {
                                return wait_for_startup(rx, Duration::from_secs(5)).await;
                            }
                            // Another model is starting — wait for it to finish, then retry
                            let _ = wait_for_startup(rx, Duration::from_secs(5)).await;
                        }
                        StartupDisposition::Initiator { guard, self_rx } => {
                            drive(guard, async move {
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                Ok(target_a.clone())
                            });
                            return wait_for_startup(self_rx, Duration::from_secs(5)).await;
                        }
                    }
                }
            },
            async {
                // Small delay so model-a claims the slot first
                tokio::time::sleep(Duration::from_millis(10)).await;
                loop {
                    let disp = StartupDisposition::check(&slot, "model-b".to_string());
                    match disp {
                        StartupDisposition::Waiter {
                            rx,
                            target_model_name,
                        } => {
                            if target_model_name == "model-b" {
                                return wait_for_startup(rx, Duration::from_secs(5)).await;
                            }
                            // Another model is starting — wait for it to finish, then retry
                            let _ = wait_for_startup(rx, Duration::from_secs(5)).await;
                        }
                        StartupDisposition::Initiator { guard, self_rx } => {
                            drive(guard, async move {
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                Ok(target_b.clone())
                            });
                            return wait_for_startup(self_rx, Duration::from_secs(5)).await;
                        }
                    }
                }
            }
        );

        // Each should get its OWN model's target, not the other's
        assert!(
            result_a.is_ok(),
            "Model A caller should succeed: {:?}",
            result_a
        );
        assert!(
            result_b.is_ok(),
            "Model B caller should succeed: {:?}",
            result_b
        );

        let a = result_a.unwrap();
        let b = result_b.unwrap();

        assert_eq!(
            a.model_name, "model-a",
            "Caller A should get model-a, got {}",
            a.model_name
        );
        assert_eq!(
            b.model_name, "model-b",
            "Caller B should get model-b, got {}",
            b.model_name
        );
        assert_ne!(
            a.model_name, b.model_name,
            "Each caller should get different models"
        );
    }
}
