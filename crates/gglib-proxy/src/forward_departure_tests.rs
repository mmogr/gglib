//! A client that leaves before the upstream's first generated token ends the
//! turn at once, as a departure and not a stall, and its KV cache is not saved;
//! a turn that generates nothing while its client stays is an empty response,
//! and is saved. After that token, be it text, reasoning or a tool call, a
//! departure is seen at the next frame sent, so a reply still being written
//! ends there and is saved, and a silence still ends in a stall at the idle
//! bound.

use std::time::{Duration, Instant};

use serde_json::json;

use super::forward_stall_fixtures::{
    Reader, Then, at_once, done, finish, frame, progress, run_turn, text, upstream,
};
use super::*;
use crate::upstream_read::upstream_events;

/// An idle bound none of the tests that use it waits out.
const LONG_IDLE: Duration = Duration::from_secs(60);

/// How long the client stays after its last frame before it leaves.
const LINGER: Duration = Duration::from_millis(200);

/// How soon after the client leaves the drain must have ended, where it ends
/// on the departure.
const NOTICED_WITHIN: Duration = Duration::from_secs(2);

/// What the drain made of a turn its client left, and how long after the
/// client left it ended.
struct Departure {
    outcome: StreamOutcome,
    noticed_after: Duration,
    /// Every byte the client took before it left.
    wire: String,
}

/// Run the drain over `bytes` under `idle`. The client takes `frames` frames,
/// waits [`LINGER`], and leaves.
async fn leave_after(
    frames: usize,
    bytes: impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    dialect: Option<DialectSpec>,
    idle: Duration,
) -> Departure {
    let registry = Arc::new(crate::connections::ActiveConnectionsRegistry::new());
    let connection = registry.register("m", true, None);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(32);

    let client = tokio::spawn(async move {
        let mut wire = String::new();
        for _ in 0..frames {
            let Some(Ok(chunk)) = rx.recv().await else {
                break;
            };
            wire.push_str(&String::from_utf8_lossy(&chunk));
        }
        tokio::time::sleep(LINGER).await;
        drop(rx);
        (wire, Instant::now())
    });

    let drain = drain_events(
        upstream_events(bytes, idle),
        "m".to_owned(),
        dialect,
        tx,
        &connection,
        None,
        false,
    );
    // A drain that does not end fails the test instead of waiting out a
    // long bound.
    let outcome = tokio::time::timeout(NOTICED_WITHIN * 5, drain)
        .await
        .expect("the drain ends");
    let ended = Instant::now();
    let (wire, left) = client.await.expect("the client task does not panic");
    Departure {
        outcome,
        noticed_after: ended.saturating_duration_since(left),
        wire,
    }
}

/// Record `outcome`'s verdict on a fresh watchdog and return it.
fn watchdog_after(outcome: &StreamOutcome) -> UpstreamHealth {
    let health = UpstreamHealth::new();
    health.record_stream_outcome(outcome.health_verdict());
    health
}

/// Check that a turn whose upstream generated and then went silent under
/// `idle` ended in the stall, not the departure, and asked for a recycle.
fn assert_the_stall_ended_it(left: &Departure, idle: Duration) {
    assert_eq!(
        left.outcome.upstream_stalled,
        Some(UpstreamStalled {
            after: idle,
            after_first_token: true,
        }),
        "{}",
        left.wire
    );
    assert!(left.outcome.client_aborted, "the notice found nobody");
    assert!(!left.outcome.left_before_first_token);
    assert_eq!(
        left.outcome.health_verdict(),
        StreamVerdict::Stalled {
            after_first_token: true
        }
    );
    assert!(watchdog_after(&left.outcome).recycle_pending());
}

#[tokio::test]
async fn a_client_that_leaves_during_a_silent_prefill_ends_the_turn_at_once_as_a_departure() {
    // Prefill has begun and gone quiet; the client, which did not ask for
    // progress frames, has been sent nothing.
    let silent_prefill = upstream(at_once(vec![progress()]), Then::Silence);

    let left = leave_after(0, silent_prefill, None, LONG_IDLE).await;

    assert!(
        left.noticed_after < NOTICED_WITHIN,
        "noticed {:?} after the client left",
        left.noticed_after
    );
    assert!(left.outcome.client_aborted);
    assert!(left.outcome.left_before_first_token);
    assert!(!left.outcome.worth_saving());
    assert_eq!(left.outcome.upstream_stalled, None);
    assert!(!left.outcome.upstream_errored);
    assert_eq!(left.outcome.health_verdict(), StreamVerdict::ClientAborted);
    let health = watchdog_after(&left.outcome).snapshot();
    assert_eq!(health.total_client_aborts, 1);
    assert_eq!(health.total_stream_stalls, 0);
    assert_eq!(health.consecutive_strikes, 0);
    assert!(left.wire.is_empty(), "{}", left.wire);
}

#[tokio::test]
async fn a_turn_that_generates_nothing_while_its_client_stays_is_an_empty_response_and_saved() {
    let nothing = upstream(at_once(vec![finish(), done()]), Then::Close);

    let turn = run_turn(nothing, None, Reader::default()).await;

    assert!(!turn.outcome.client_aborted, "{}", turn.wire);
    assert!(!turn.outcome.left_before_first_token);
    assert!(turn.outcome.worth_saving());
    assert_eq!(turn.outcome.health_verdict(), StreamVerdict::Empty);
}

#[tokio::test]
async fn a_client_that_stops_a_reply_still_being_written_is_seen_at_the_next_frame_and_saved() {
    let talking: Vec<_> = (0..40)
        .map(|_| (Duration::from_millis(50), text("word ")))
        .collect();

    let left = leave_after(2, upstream(talking, Then::Close), None, LONG_IDLE).await;

    assert!(
        left.noticed_after < NOTICED_WITHIN,
        "noticed {:?} after the client left",
        left.noticed_after
    );
    assert!(left.outcome.client_aborted);
    assert!(!left.outcome.left_before_first_token);
    assert!(left.outcome.worth_saving());
    assert_eq!(left.outcome.health_verdict(), StreamVerdict::Healthy);
}

#[tokio::test]
async fn a_client_that_leaves_while_a_tool_call_is_held_back_still_waits_for_the_stall() {
    // A Qwen model writes its call as text, which the normalizer holds back
    // until it is whole: the drain has seen the model generate, but has sent
    // the client nothing, when the upstream goes silent and the client leaves.
    let markup =
        ["<tool_call>\n", "<function=read_file>\n"].map(|m| frame(&json!({ "content": m }), None));
    let idle = LINGER * 5;

    let left = leave_after(
        0,
        upstream(at_once(markup.to_vec()), Then::Silence),
        Some(DialectSpec::qwen_xml()),
        idle,
    )
    .await;

    assert_the_stall_ended_it(&left, idle);
}

#[tokio::test]
async fn a_client_that_leaves_a_reply_silent_after_its_reasoning_still_waits_for_the_stall() {
    let reasoning = frame(&json!({ "reasoning_content": "Let me think" }), None);
    let idle = LINGER * 5;

    let left = leave_after(
        1,
        upstream(at_once(vec![reasoning]), Then::Silence),
        None,
        idle,
    )
    .await;

    // The frame the client took was the reply's, not the stall's notice.
    assert!(left.wire.contains("Let me think"), "{}", left.wire);
    assert_the_stall_ended_it(&left, idle);
}

#[tokio::test]
async fn a_client_that_leaves_a_reply_silent_after_a_native_tool_call_still_waits_for_the_stall() {
    let call = json!({"tool_calls": [{
        "index": 0,
        "id": "call_1",
        "type": "function",
        "function": {"name": "read_file", "arguments": "{\"path\": \"src/"},
    }]});
    let idle = LINGER * 5;

    let left = leave_after(
        1,
        upstream(at_once(vec![frame(&call, None)]), Then::Silence),
        None,
        idle,
    )
    .await;

    // The frame the client took was the reply's, not the stall's notice.
    assert!(left.wire.contains("read_file"), "{}", left.wire);
    assert_the_stall_ended_it(&left, idle);
}
