//! What a streamed turn is counted and timed as having generated: the four
//! readings the drain reports, and the [`TurnGeneration`] a snapshot is given.
//!
//! The drain is fed llama-server's frames, each after a wait of its own. The
//! timed tests pause tokio's clock, so a wait takes exactly its length and
//! nothing else takes any.
//!
//! The time is taken as frames arrive from the upstream.

use std::time::Duration;

use futures_util::stream;

use super::forward_generation_fixtures::{
    Client, call_opens, done, drain, drain_stream, full_turn, full_turn_generation, holding,
    reasoning, upstream, usage,
};
use super::*;
use crate::generation::TurnGeneration;

#[tokio::test(start_paused = true)]
async fn a_streamed_turn_is_counted_and_timed() {
    let outcome = drain(full_turn(), None, Client::Stays).await;

    assert_eq!(outcome.completion_tokens, Some(37));
    assert_eq!(outcome.reasoning_chars, 12, "characters, not bytes");
    assert_eq!(outcome.tool_calls, 2, "calls, not their four fragments");
    assert_eq!(
        outcome.generation_ms,
        Some(130),
        "first token to last: not the 120 ms before, the last 60 of them \
         after a prefill frame, nor the 250 ms after"
    );
    assert_eq!(TurnGeneration::of_stream(&outcome), full_turn_generation());
}

/// With the hold-back engaged the calls reach the client at `Done`, and the
/// turn reads the same: timed as generated, counted as sent.
#[tokio::test(start_paused = true)]
async fn a_turn_whose_calls_are_held_back_reads_the_same() {
    let outcome = drain(full_turn(), Some(holding()), Client::Stays).await;

    assert_eq!(TurnGeneration::of_stream(&outcome), full_turn_generation());
}

/// A held call is not a sent one: the client leaves while its frame is held,
/// and the turn records the reasoning it read and no call.
#[tokio::test(start_paused = true)]
async fn a_call_still_held_when_the_client_leaves_is_not_counted() {
    let events = vec![
        (0, call_opens(0, "read_file")),
        (50, reasoning("abc")),
        (50, done("tool_calls")),
        (0, usage(37)),
    ];
    let left = Client::Leaves { after: 0, wait: 20 };
    let outcome = drain(events, Some(holding()), left).await;

    assert!(outcome.client_aborted);
    assert_eq!(
        TurnGeneration::of_stream(&outcome),
        TurnGeneration {
            completion_tokens: None,
            reasoning_chars: 3,
            tool_calls: 0,
            finish_reason: None,
            generation_ms: Some(50),
        },
        "what was read before the client went, and nothing after"
    );
}

/// Unheld, a call counts once its frame is sent: the client takes the first
/// call and is gone before the second.
#[tokio::test(start_paused = true)]
async fn a_call_the_client_left_before_is_not_counted() {
    let events = vec![
        (0, call_opens(0, "read_file")),
        (50, call_opens(1, "list_dir")),
        (50, done("tool_calls")),
    ];
    let left = Client::Leaves { after: 1, wait: 20 };
    let outcome = drain(events, None, left).await;

    assert!(outcome.client_aborted);
    assert_eq!(outcome.tool_calls, 1, "the call sent, not the one read");
    assert_eq!(outcome.generation_ms, Some(50), "both were generated");
}

/// Held calls are counted as each is sent: the client takes the first of
/// two released at `Done` and hangs up, so the second is never sent.
#[tokio::test(start_paused = true)]
async fn a_released_call_the_client_left_before_is_not_counted() {
    let events = vec![
        (0, call_opens(0, "read_file")),
        (0, call_opens(1, "list_dir")),
        (0, done("tool_calls")),
    ];
    let left = Client::Leaves { after: 1, wait: 0 };
    let outcome = drain(events, Some(holding()), left).await;

    assert!(outcome.client_aborted);
    assert_eq!(outcome.tool_calls, 1, "the call sent, not the two held");
}

/// A stream that ends without `Done` still releases its held call, and the
/// call is counted.
#[tokio::test(start_paused = true)]
async fn a_held_call_released_without_a_done_is_counted() {
    let events = vec![(0, call_opens(0, "read_file"))];
    let outcome = drain(events, Some(holding()), Client::Stays).await;

    assert_eq!(
        TurnGeneration::of_stream(&outcome),
        TurnGeneration {
            completion_tokens: None,
            reasoning_chars: 0,
            tool_calls: 1,
            finish_reason: None,
            generation_ms: Some(0),
        },
        "one delta spans no time, and no Done or usage frame was read"
    );
}

/// Three characters of reasoning, then, 30 ms on, a call the drain holds.
fn reasoning_then_a_held_call() -> Vec<(u64, LlmStreamEvent)> {
    vec![(0, reasoning("abc")), (30, call_opens(0, "read_file"))]
}

/// What a turn of [`reasoning_then_a_held_call`] generated when it died
/// upstream after them and its held call was released to the client.
fn a_released_call_on_a_dead_turn() -> TurnGeneration {
    TurnGeneration {
        completion_tokens: None,
        reasoning_chars: 3,
        tool_calls: 1,
        finish_reason: None,
        generation_ms: Some(30),
    }
}

/// An upstream error event ends the turn with no `Done`. The held call is
/// released ahead of the error frame, and counted; the error is not a token,
/// so the wait before it is not in the time.
#[tokio::test(start_paused = true)]
async fn a_held_call_released_ahead_of_an_upstream_error_is_counted() {
    let mut events = reasoning_then_a_held_call();
    events.push((
        70,
        LlmStreamEvent::UpstreamError {
            message: "slot unavailable".to_owned(),
            error_type: "server_error".to_owned(),
            code: "upstream_error".to_owned(),
        },
    ));
    let outcome = drain(events, Some(holding()), Client::Stays).await;

    assert!(outcome.upstream_errored);
    assert!(!outcome.client_aborted);
    assert_eq!(
        TurnGeneration::of_stream(&outcome),
        a_released_call_on_a_dead_turn()
    );
}

/// The upstream's byte stream breaks, which the drain reads as an `Err`, not
/// an event. The held call is released ahead of the error frame, and counted.
#[tokio::test(start_paused = true)]
async fn a_held_call_released_when_the_upstream_stream_breaks_is_counted() {
    let breaks = stream::once(async {
        tokio::time::sleep(Duration::from_millis(70)).await;
        Err(std::io::Error::other("connection reset"))
    });
    let events = upstream(reasoning_then_a_held_call()).chain(breaks);
    let outcome = drain_stream(events, Some(holding()), Client::Stays).await;

    assert!(outcome.upstream_errored);
    assert!(!outcome.client_aborted);
    assert_eq!(
        TurnGeneration::of_stream(&outcome),
        a_released_call_on_a_dead_turn()
    );
}

/// A turn with no delta has no span, which is not a span of zero.
#[tokio::test(start_paused = true)]
async fn a_turn_that_generated_nothing_has_no_generation_time() {
    let events = vec![(80, done("stop")), (20, usage(0))];
    let outcome = drain(events, None, Client::Stays).await;

    assert_eq!(
        TurnGeneration::of_stream(&outcome),
        TurnGeneration {
            completion_tokens: Some(0),
            reasoning_chars: 0,
            tool_calls: 0,
            finish_reason: Some("stop".to_owned()),
            generation_ms: None,
        }
    );
}
