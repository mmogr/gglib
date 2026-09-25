//! What a stalled turn tells the watchdog, and what it is not worth.
//!
//! A stall after the first token asks for a recycle at once; one before it,
//! which a slow prefill can cause, only strikes. A stall after the turn's
//! `Done` is quiet on the wire but still a stall, and a client that leaves
//! during a silence after the first token does not turn the stall into its
//! own doing.

use serde_json::json;

use super::forward_stall_fixtures::{
    IDLE, Reader, Then, at_once, finish, frame, progress, run_turn, text, upstream,
};
use super::*;

/// Record `outcome`'s verdict on a fresh watchdog and return it.
fn watchdog_after(outcome: &StreamOutcome) -> UpstreamHealth {
    let health = UpstreamHealth::new();
    health.record_stream_outcome(outcome.health_verdict());
    health
}

#[tokio::test]
async fn a_stall_after_the_finish_ends_quietly_but_still_strikes() {
    // The answer and its finish arrived; the usage frame and `[DONE]` never
    // did.
    let chunks = at_once(vec![text("Hi"), finish()]);
    let turn = run_turn(upstream(chunks, Then::Silence), None, Reader::default()).await;

    assert_eq!(turn.text(), "Hi", "no notice after a complete answer");
    assert!(turn.error_codes().is_empty(), "{}", turn.wire);
    assert_eq!(turn.dones(), 1);
    assert_eq!(turn.outcome.finish_reason.as_deref(), Some("stop"));

    assert!(turn.outcome.upstream_stalled.is_some());
    let health = watchdog_after(&turn.outcome);
    assert_eq!(health.snapshot().total_stream_stalls, 1);
    assert_eq!(health.snapshot().consecutive_strikes, 1);
}

#[tokio::test]
async fn a_stall_outranks_a_client_that_left_during_it() {
    let chunks = at_once(vec![text("Hel")]);
    let leaves = Reader {
        leaves_after: Some(1),
        ..Reader::default()
    };

    let turn = run_turn(upstream(chunks, Then::Silence), None, leaves).await;

    assert!(turn.outcome.client_aborted, "the notice found nobody");
    assert!(turn.outcome.upstream_stalled.is_some());
    assert_eq!(
        turn.outcome.health_verdict(),
        StreamVerdict::Stalled {
            after_first_token: true
        }
    );
    let health = watchdog_after(&turn.outcome);
    assert_eq!(health.snapshot().total_client_aborts, 0);
    assert!(health.recycle_pending());
}

#[tokio::test]
async fn a_stalled_turn_is_not_worth_saving_and_a_finished_one_is() {
    let stalled = run_turn(
        upstream(at_once(vec![text("Hel")]), Then::Silence),
        None,
        Reader::default(),
    )
    .await;
    let finished = run_turn(
        upstream(at_once(vec![text("Hello"), finish()]), Then::Close),
        None,
        Reader::default(),
    )
    .await;

    assert!(!stalled.outcome.worth_saving());
    assert!(finished.outcome.worth_saving());
}

#[tokio::test]
async fn a_stall_before_the_first_token_strikes_without_a_recycle() {
    // Prefill had begun and then nothing more came: on a slow host this is
    // what a slow prefill looks like, so it must not recycle on sight.
    let turn = run_turn(
        upstream(at_once(vec![progress()]), Then::Silence),
        None,
        Reader::default(),
    )
    .await;

    let stall = turn
        .outcome
        .upstream_stalled
        .expect("the silence is a stall");
    assert_eq!(stall.after, IDLE);
    assert!(!stall.after_first_token);
    // The notice is the turn's whole text, with no empty-turn notice after
    // it, and the error frame is the last thing before `[DONE]`.
    assert_eq!(turn.text(), stall.notice());
    assert!(
        turn.text().contains("went silent") && !turn.text().contains("recycled"),
        "the notice says what happened and promises no recycle: {}",
        turn.text()
    );
    assert_eq!(turn.error_codes(), ["upstream_timeout"]);
    let payloads = turn.payloads();
    let [.., error, sentinel] = payloads.as_slice() else {
        panic!("too few frames: {}", turn.wire);
    };
    assert!(error.contains("upstream_timeout"), "{}", turn.wire);
    assert_eq!(*sentinel, "[DONE]");

    let health = watchdog_after(&turn.outcome);
    assert_eq!(health.snapshot().consecutive_strikes, 1);
    assert_eq!(health.snapshot().total_stream_stalls, 1);
    assert!(!health.recycle_pending());
}

#[tokio::test]
async fn a_stall_after_reasoning_alone_is_after_the_first_token() {
    // Reasoning is generated output: prefill was over, so this silence cannot
    // be a slow prefill, even though the client saw nothing it would render.
    let chunks = at_once(vec![frame(&json!({ "reasoning_content": "Let me" }), None)]);
    let turn = run_turn(upstream(chunks, Then::Silence), None, Reader::default()).await;

    let stall = turn
        .outcome
        .upstream_stalled
        .expect("the silence is a stall");
    assert!(stall.after_first_token);
    assert!(watchdog_after(&turn.outcome).recycle_pending());
}
