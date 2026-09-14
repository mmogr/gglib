//! A child of `enable_race_tests.rs`, for the failure its settings double has
//! to be told to make: a write of the switch off, once the `enable`'s write of
//! it on has landed.

use super::*;

/// A `disable` lands while the `enable`'s write of the switch is parked, and
/// the `enable`'s write of the switch back off then fails. The switch is still
/// on for the next start, so the `enable` says so and names the command,
/// rather than reporting a clean cancellation.
#[tokio::test]
async fn an_enable_that_cannot_put_the_switch_back_off_says_so() {
    let (core, gate, ops, _arming) = ops_with_a_gated_switch(true).await;
    gate.refuse_off.store(true, Ordering::SeqCst);
    let enabling = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.enable(offline()).await }
    });
    let parked = tokio::time::timeout(STARTED, gate.reached.notified()).await;
    let disabled = ops.disable().await;
    gate.opened.notify_one();
    let enabled = enabling.await.expect("the enable task");
    let switched = switch(&core).await;
    gate.refuse_off.store(false, Ordering::SeqCst);
    // Take down anything left, and switch it off, before judging.
    let _ = ops.disable().await;

    assert!(parked.is_ok(), "the enable never wrote the switch");
    assert!(
        disabled.is_ok(),
        "the disable found the reservation and cancelled it"
    );
    assert_eq!(
        switched,
        Some(true),
        "the store's refusal left the switch on"
    );
    let Err(GuiError::Internal(message)) = enabled else {
        panic!("an enable that could not switch back off says so: {enabled:?}");
    };
    assert!(message.contains("gglib remote disable"), "{message}");
}

/// An `enable` arms, and then the store refuses the `disable`'s write of the
/// switch off. The tunnel comes down all the same, but the switch is still on
/// for the next start, so the `disable` says so and names the command, rather
/// than reporting success.
#[tokio::test]
async fn a_disable_that_cannot_switch_it_off_says_so() {
    let (core, gate, ops, _arming) = ops_with_a_gated_switch(true).await;
    let enabling = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.enable(offline()).await }
    });
    let parked = tokio::time::timeout(STARTED, gate.reached.notified()).await;
    gate.opened.notify_one();
    let enabled = enabling.await.expect("the enable task");
    gate.refuse_off.store(true, Ordering::SeqCst);
    let disabled = ops.disable().await;
    let up = ops.status().await.enabled;
    let switched = switch(&core).await;
    gate.refuse_off.store(false, Ordering::SeqCst);
    // Take down anything left, and switch it off, before judging.
    let _ = ops.disable().await;

    assert!(parked.is_ok(), "the enable never wrote the switch");
    assert!(enabled.is_ok(), "the enable armed: {enabled:?}");
    assert!(!up, "the disable took the tunnel down all the same");
    assert_eq!(
        switched,
        Some(true),
        "the store's refusal left the switch on"
    );
    let Err(GuiError::Internal(message)) = disabled else {
        panic!("a disable that could not switch it off says so: {disabled:?}");
    };
    assert!(message.contains("gglib remote disable"), "{message}");
}
