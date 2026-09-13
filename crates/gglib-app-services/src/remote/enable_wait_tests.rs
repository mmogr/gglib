//! Tests for `enable` and `invite` waiting out the daemon's own resume
//! instead of being refused by it ([#1037]).
//!
//! On `test_remote_ops()`'s fixture, whose proxy is real, so nothing here may
//! reach `ensure_running`. The resume is stood in for by holding its guard,
//! and every call below waits, is cancelled, or meets a slot that refuses it
//! before it would start the proxy.
//!
//! [#1037]: https://github.com/mmogr/gglib/issues/1037

use std::sync::Arc;
use std::time::Duration;

use super::serve_switch::CANCELLED_BY_DISABLE;
use super::*;
use crate::error::GuiError;
use crate::test_support_remote::test_remote_ops;

/// Long enough for a call that is not going to wait to have answered.
const ANSWERED: Duration = Duration::from_millis(500);

/// The flag is state anyone can read, not a message only a waiter hears:
/// set and cleared with nobody subscribed, a fresh subscriber sees each.
#[tokio::test]
async fn the_resume_flag_is_readable_with_nobody_waiting_on_it() {
    let (_, ops, _) = test_remote_ops().await;
    let held = ops.mark_resuming();
    assert!(
        *ops.resuming.subscribe().borrow(),
        "a resume under way does not read as one"
    );
    drop(held);
    assert!(
        !*ops.resuming.subscribe().borrow(),
        "a resume that ended still reads as working"
    );
}

/// The window the report landed in: the resume has started and is still
/// starting the proxy, so the slot is empty. An `enable` then has to wait,
/// not go on to start the proxy itself and lose the race to reserve.
///
/// The slot is reserved before the resume ends, so what the `enable` does
/// next is refuse, before it would start the proxy this fixture must not
/// reach. What this shows is that it waited until then.
#[tokio::test]
async fn an_enable_waits_while_a_resume_is_working_even_before_it_has_reserved_the_slot() {
    let (_, ops, _) = test_remote_ops().await;
    let held = ops.mark_resuming();
    let mut enabling = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.enable(EnableRequest::default()).await }
    });
    assert!(
        tokio::time::timeout(ANSWERED, &mut enabling).await.is_err(),
        "an enable during a resume answered before the resume was over"
    );

    let _cancel = ops
        .live
        .lock()
        .await
        .reserve(1)
        .expect("nothing holds the serve side");
    drop(held);
    let answered = tokio::time::timeout(ANSWERED, enabling)
        .await
        .expect("the enable kept waiting after the resume was over")
        .expect("the enable task");
    let Err(GuiError::Conflict(message)) = answered else {
        panic!("a slot still reserved is refused: {answered:?}");
    };
    assert!(message.contains("already being enabled"), "{message}");
}

/// The wait is for the daemon's own resume and nothing else. A person's
/// second `enable` while their first is arming is refused at once, as
/// `a_second_enable_while_one_is_arming_says_a_ticket_is_on_its_way` has it.
#[tokio::test]
async fn a_second_enable_while_a_person_is_arming_still_refuses_instead_of_waiting() {
    let (_, ops, _) = test_remote_ops().await;
    let _cancel = ops
        .live
        .lock()
        .await
        .reserve(1)
        .expect("nothing holds the serve side");
    let answered = tokio::time::timeout(ANSWERED, ops.enable(EnableRequest::default()))
        .await
        .expect("a second enable waited on an arming that is not a resume");
    let Err(GuiError::Conflict(message)) = answered else {
        panic!("an arming already under way is a conflict: {answered:?}");
    };
    assert!(message.contains("already being enabled"), "{message}");
}

/// A `disable` while an `enable` waits wins. The `enable` gives up rather
/// than arming, once the resume is over, a tunnel the person has just turned
/// off.
#[tokio::test]
async fn a_disable_while_an_enable_waits_out_the_resume_wins() {
    let (core, ops, _) = test_remote_ops().await;
    let held = ops.mark_resuming();
    let mut enabling = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.enable(EnableRequest::default()).await }
    });
    assert!(
        tokio::time::timeout(ANSWERED, &mut enabling).await.is_err(),
        "an enable during a resume answered before the resume was over"
    );

    // Nothing is bound here, so `disable` finds nothing to take down. What
    // it is for in this test is the switch and the waiting `enable`.
    let _ = ops.disable().await;
    let answered = tokio::time::timeout(ANSWERED, enabling)
        .await
        .expect("the enable kept waiting after a disable")
        .expect("the enable task");
    drop(held);
    let switched_on = core
        .settings()
        .get()
        .await
        .expect("settings")
        .remote_enabled;

    let Err(GuiError::Conflict(message)) = answered else {
        panic!("a disable while waiting cancels the enable: {answered:?}");
    };
    assert_eq!(message, CANCELLED_BY_DISABLE);
    assert_eq!(switched_on, Some(false), "the switch is off");
}

/// `invite` waits too, so it and `enable --invite` typed a few seconds apart
/// answer alike. With nothing armed when the resume ends, it is refused the
/// way it always was: after the wait, not instead of it.
#[tokio::test]
async fn an_invite_during_a_resume_waits_for_it_instead_of_refusing_at_once() {
    let (_, ops, _) = test_remote_ops().await;
    let held = ops.mark_resuming();
    let mut inviting = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.invite().await }
    });
    assert!(
        tokio::time::timeout(ANSWERED, &mut inviting).await.is_err(),
        "an invite during a resume was refused before the resume was over"
    );
    drop(held);
    let answered = tokio::time::timeout(ANSWERED, inviting)
        .await
        .expect("the invite kept waiting after the resume was over")
        .expect("the invite task");
    assert!(
        matches!(answered, Err(GuiError::Conflict(_))),
        "nothing was armed, so there is nothing to invite onto: {answered:?}"
    );
}

/// A `disable` while an `invite` waits ends the invite at once. The slot may
/// still read as arming until that `disable` reaches it, and a code offered
/// on a session about to go would leave a row nobody can redeem.
#[tokio::test]
async fn an_invite_waiting_out_a_resume_gives_up_when_a_disable_lands() {
    let (_, ops, _) = test_remote_ops().await;
    let held = ops.mark_resuming();
    let mut inviting = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.invite().await }
    });
    assert!(
        tokio::time::timeout(ANSWERED, &mut inviting).await.is_err(),
        "an invite during a resume was refused before the resume was over"
    );
    let _ = ops.disable().await;
    let answered = tokio::time::timeout(ANSWERED, inviting)
        .await
        .expect("the invite kept waiting after a disable")
        .expect("the invite task");
    drop(held);
    let Err(GuiError::Conflict(message)) = answered else {
        panic!("a disable while waiting cancels the invite: {answered:?}");
    };
    assert!(
        message.contains("cancelled by `gglib remote disable`"),
        "{message}"
    );
}
