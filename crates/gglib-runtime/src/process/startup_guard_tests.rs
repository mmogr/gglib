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
                    drive(guard, Duration::from_millis(500), async move {
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
                    drive(guard, Duration::from_millis(500), async move {
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
                    drive(guard, Duration::from_millis(500), async move {
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
                    drive(guard, Duration::from_millis(500), async move {
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
                drive(guard, Duration::from_millis(500), async move {
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
                drive(guard, Duration::from_millis(500), async move {
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
                drive(guard, Duration::from_millis(200), async move {
                    // Never returns — simulates stuck driver (but internal timeout will kill it)
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
        matches!(&result, Err(ModelRuntimeError::Internal(msg)) if msg.contains("timed out") || msg.contains("deadline")),
        "Should get timeout or deadline error, got: {:?}",
        result
    );

    // Driver now self-terminates on internal timeout — verify slot clears via bounded retry
    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    loop {
        if let Ok(guard) = slot.read()
            && guard.is_none()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(
            tokio::time::Instant::now() < deadline,
            "Slot not cleared within 500ms after driver timeout"
        );
    }
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
                        drive(guard, Duration::from_millis(500), async move {
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
                        drive(guard, Duration::from_millis(500), async move {
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

/// Test 8: Driver internal timeout kills the driver and clears the slot.
#[tokio::test]
async fn test_driver_internal_timeout_clears_slot() {
    let slot = Arc::new(RwLock::new(None));

    // Driver sleeps 800ms but its timeout is only 500ms — it should self-terminate
    {
        let disp = StartupDisposition::check(&slot, "slow-model".to_string());
        match disp {
            StartupDisposition::Initiator { guard, self_rx: _ } => {
                drive(guard, Duration::from_millis(500), async move {
                    tokio::time::sleep(Duration::from_millis(800)).await;
                    Ok(RunningTarget::local(
                        8080,
                        1,
                        "slow-model".to_string(),
                        2048,
                    ))
                });
                // Don't await self_rx — let the driver timeout handle cleanup
            }
            _ => unreachable!(),
        }
    }

    // Waiter should get the driver's deadline error (not a generic timeout)
    tokio::time::sleep(Duration::from_millis(50)).await;
    let result = {
        let disp = StartupDisposition::check(&slot, "slow-model".to_string());
        match disp {
            StartupDisposition::Waiter { rx, .. } => {
                wait_for_startup(rx, Duration::from_secs(5)).await
            }
            _ => unreachable!(),
        }
    };

    assert!(
        matches!(&result, Err(ModelRuntimeError::Internal(msg)) if msg.contains("deadline")),
        "Should get driver deadline error, got: {:?}",
        result
    );

    // Slot should be cleared (driver self-terminated and cleaned up)
    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    loop {
        if let Ok(guard) = slot.read()
            && guard.is_none()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(
            tokio::time::Instant::now() < deadline,
            "Slot not cleared within 500ms after driver internal timeout"
        );
    }
}

#[test]
fn test_cross_model_waiter_bails_on_insufficient_budget() {
    // Below threshold — should bail
    assert!(
        super::should_bail_on_insufficient_budget(Duration::from_secs(74)),
        "74s < 75s MIN_STARTUP_BUDGET → should bail"
    );
    // At threshold — should NOT bail (equal is OK)
    assert!(
        !super::should_bail_on_insufficient_budget(Duration::from_secs(75)),
        "75s == 75s MIN_STARTUP_BUDGET → should NOT bail"
    );
    // Well above threshold — should NOT bail
    assert!(
        !super::should_bail_on_insufficient_budget(Duration::from_secs(100)),
        "100s > 75s MIN_STARTUP_BUDGET → should NOT bail"
    );
}
