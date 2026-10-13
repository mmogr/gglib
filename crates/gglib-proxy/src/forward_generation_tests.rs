//! What a streamed turn is counted and timed as having generated: the four
//! readings the drain reports, and the [`TurnGeneration`] a snapshot is given.
//!
//! The drain is fed events, not bytes, each after a wait of its own. The
//! timed tests pause tokio's clock, so a wait takes exactly its length and
//! nothing else takes any.
//!
//! The time is taken as events arrive from the upstream, before the
//! normalizer, and the Qwen tests are the ones that tell the two sides
//! apart: that model's call arrives as text over seconds and leaves the
//! normalizer as one event.

use std::time::Duration;

use futures_util::stream::{self, Stream};
use serde_json::json;

use super::*;
use crate::generation::TurnGeneration;

fn reasoning(content: &str) -> LlmStreamEvent {
    LlmStreamEvent::ReasoningDelta {
        content: content.to_owned(),
    }
}

fn text(content: &str) -> LlmStreamEvent {
    LlmStreamEvent::TextDelta {
        content: content.to_owned(),
    }
}

/// The first fragment of the call at `index`: its id and name, no arguments.
fn call_opens(index: usize, name: &str) -> LlmStreamEvent {
    LlmStreamEvent::ToolCallDelta {
        index,
        id: Some(format!("call_{index}")),
        name: Some(name.to_owned()),
        arguments: None,
    }
}

/// A later fragment of the call at `index`: arguments only.
fn call_continues(index: usize, arguments: &str) -> LlmStreamEvent {
    LlmStreamEvent::ToolCallDelta {
        index,
        id: None,
        name: None,
        arguments: Some(arguments.to_owned()),
    }
}

fn done(finish_reason: &str) -> LlmStreamEvent {
    LlmStreamEvent::Done {
        finish_reason: Some(finish_reason.to_owned()),
    }
}

/// A usage frame whose three counts differ, so one read for another shows.
fn usage(completion_tokens: u32) -> LlmStreamEvent {
    LlmStreamEvent::Usage {
        prompt_tokens: 11,
        completion_tokens,
        total_tokens: 11 + completion_tokens,
        cached_tokens: None,
    }
}

/// An upstream that sends each event once its wait, in milliseconds, is over.
fn upstream(
    events: Vec<(u64, LlmStreamEvent)>,
) -> impl Stream<Item = anyhow::Result<LlmStreamEvent>> + Send + 'static {
    stream::iter(events).then(|(wait, event)| async move {
        tokio::time::sleep(Duration::from_millis(wait)).await;
        Ok(event)
    })
}

/// A prefill progress frame, which is not a generated token.
fn prefill() -> LlmStreamEvent {
    LlmStreamEvent::PromptProgress {
        processed: 10,
        total: 57,
        cached: 0,
        time_ms: 5,
    }
}

/// A turn of every kind of event: a prefill frame in the middle of a wait,
/// twelve characters of reasoning (fourteen bytes), a call in three
/// fragments, a second call, text, and then a wait before `Done` and the
/// usage frame. The generated tokens span 130 ms.
fn full_turn() -> Vec<(u64, LlmStreamEvent)> {
    vec![
        (60, prefill()),
        (60, reasoning("plan: ré")),
        (40, reasoning("sumé")),
        (30, call_opens(0, "read_file")),
        (10, call_continues(0, "{\"path\":")),
        (10, call_continues(0, "\"a\"}")),
        (25, call_opens(1, "list_dir")),
        (15, text("done")),
        (200, done("tool_calls")),
        (50, usage(37)),
    ]
}

/// What [`full_turn`] generated.
fn full_turn_generation() -> TurnGeneration {
    TurnGeneration {
        completion_tokens: Some(37),
        reasoning_chars: 12,
        tool_calls: 2,
        finish_reason: Some("tool_calls".to_owned()),
        generation_ms: Some(130),
    }
}

/// A repair context that never asks again: repair is off, so held frames are
/// released as they are at `Done`.
fn holding() -> RepairContext {
    RepairContext {
        req_builder: crate::loopback::client().post("http://127.0.0.1:9/"),
        request_body: Bytes::new(),
        turn: RepairTurn::OFF,
    }
}

/// How the client reads the turn.
#[derive(Clone, Copy)]
enum Client {
    /// It takes every frame.
    Stays,
    /// It takes this many frames, waits this many milliseconds, and hangs up.
    /// With no wait it hangs up before the drain runs again.
    Leaves { after: usize, wait: u64 },
}

/// [`drain_as`] for a model with no dialect: the normalizer passes every
/// event through as it arrives.
async fn drain(
    events: Vec<(u64, LlmStreamEvent)>,
    repair: Option<RepairContext>,
    client: Client,
) -> StreamOutcome {
    drain_as(None, events, repair, client).await
}

/// Drain `events`, normalized as `dialect` says, to a client that reads as
/// `client` says, through a one-frame channel: a second frame is not sent
/// until the first is taken.
async fn drain_as(
    dialect: Option<DialectSpec>,
    events: Vec<(u64, LlmStreamEvent)>,
    repair: Option<RepairContext>,
    client: Client,
) -> StreamOutcome {
    drain_stream(dialect, upstream(events), repair, client).await
}

/// [`drain_as`] over any upstream, so one that breaks can be drained too.
async fn drain_stream(
    dialect: Option<DialectSpec>,
    events: impl Stream<Item = anyhow::Result<LlmStreamEvent>> + Send + 'static,
    repair: Option<RepairContext>,
    client: Client,
) -> StreamOutcome {
    let registry = Arc::new(crate::connections::ActiveConnectionsRegistry::new());
    let connection = registry.register("m", true, None);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(1);
    let reader = tokio::spawn(async move {
        match client {
            Client::Stays => while rx.recv().await.is_some() {},
            Client::Leaves { after, wait } => {
                for _ in 0..after {
                    rx.recv().await;
                }
                if wait > 0 {
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                }
            }
        }
    });
    let outcome = drain_events(
        events,
        "m".to_owned(),
        dialect,
        ClientSender::new(tx, Duration::from_secs(5)),
        &connection,
        repair,
        None,
    )
    .await;
    reader.await.expect("the client task does not panic");
    outcome
}

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

/// A Qwen model writes its call as text, one fragment a second, the first
/// after a second's wait. The normalizer holds the text back and makes one
/// tool-call event of it.
fn a_call_written_as_text() -> Vec<(u64, LlmStreamEvent)> {
    [
        "<tool_call>\n",
        "<function=read_file>\n",
        "<parameter=path>\n",
        "src/lib.rs\n",
        "</parameter>\n",
        "</function>\n",
        "</tool_call>",
    ]
    .into_iter()
    .map(|markup| (1000, text(markup)))
    .collect()
}

/// The time is the upstream's, not the normalizer's: seven fragments span
/// six seconds, and the one event they leave the normalizer as spans none.
#[tokio::test(start_paused = true)]
async fn a_call_written_as_text_is_timed_over_its_markup() {
    let mut events = a_call_written_as_text();
    events.push((400, done("stop")));
    events.push((0, usage(37)));
    let qwen = Some(DialectSpec::qwen_xml());
    let outcome = drain_as(qwen, events, None, Client::Stays).await;

    assert_eq!(outcome.tool_calls, 1, "the markup was read as one call");
    assert_eq!(
        outcome.generation_ms,
        Some(6000),
        "first fragment to last: not 0, the span of the one event the \
         normalizer made, and not the second before or the 400 ms after"
    );
}

/// Markup that never closes leaves the normalizer as an error, whose raw
/// text the client is sent. It was generated all the same, over three
/// seconds.
#[tokio::test(start_paused = true)]
async fn markup_that_never_closes_is_timed_over_what_was_streamed() {
    let events = vec![
        (1000, text("<tool_call>\n")),
        (2500, text("<function=read_file>\n")),
        (500, text("<parameter=path>\n")),
        (400, done("stop")),
    ];
    let qwen = Some(DialectSpec::qwen_xml());
    let outcome = drain_as(qwen, events, None, Client::Stays).await;

    assert!(outcome.normalization_errored, "the markup was not a call");
    assert_eq!(outcome.tool_calls, 0);
    assert_eq!(
        outcome.generation_ms,
        Some(3000),
        "first fragment to last, though no delta left the normalizer"
    );
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
        Err(anyhow::anyhow!("connection reset"))
    });
    let events = upstream(reasoning_then_a_held_call()).chain(breaks);
    let outcome = drain_stream(None, events, Some(holding()), Client::Stays).await;

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

/// A `read_file` request as forwarded upstream, on an `auto` turn.
fn read_file_request() -> Bytes {
    let body = json!({
        "model": "m",
        "stream": true,
        "tool_choice": "auto",
        "messages": [{"role": "user", "content": "read it"}],
        "tools": [{"type": "function", "function": {"name": "read_file", "parameters": {
            "type": "object",
            "properties": {"path": {"type": "string"}, "max_lines": {"type": "integer"}},
            "required": ["path"]
        }}}]
    });
    Bytes::from(serde_json::to_vec(&body).unwrap())
}

/// A turn of one `read_file` call that lacks its required `path`, after four
/// characters of reasoning.
fn bad_call_turn() -> Vec<(u64, LlmStreamEvent)> {
    vec![
        (0, reasoning("hmm.")),
        (0, call_opens(0, "read_file")),
        (0, call_continues(0, "{\"max_lines\":42}")),
        (0, done("tool_calls")),
        (0, usage(37)),
    ]
}

/// A re-issue's answer: two conformant calls, with reasoning and a usage of
/// its own, neither of which is the first generation's.
fn two_good_calls() -> serde_json::Value {
    let call = |id: &str, path: &str| {
        json!({"id": id, "type": "function", "function": {
            "name": "read_file", "arguments": json!({"path": path}).to_string()
        }})
    };
    json!({
        "choices": [{"index": 0, "finish_reason": "stop", "message": {
            "role": "assistant",
            "reasoning_content": "a second, longer thought",
            "tool_calls": [call("call_a", "a.rs"), call("call_b", "b.rs")]
        }}],
        "usage": {"prompt_tokens": 11, "completion_tokens": 99, "total_tokens": 110}
    })
}

/// Drain [`bad_call_turn`] with repair on, the re-issue answered with
/// `status` and `answer`.
async fn drain_reissued(status: u16, answer: serde_json::Value) -> StreamOutcome {
    use axum::routing::post;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().unwrap().port();
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let body = answer.to_string();
            async move {
                axum::response::Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap()
            }
        }),
    );
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let repair = RepairContext {
        req_builder: crate::loopback::client()
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions")),
        request_body: read_file_request(),
        turn: RepairTurn::ON,
    };
    let outcome = drain(bad_call_turn(), Some(repair), Client::Stays).await;
    server.abort();
    outcome
}

/// A repaired turn counts the calls that replaced the original, and keeps
/// the first generation's tokens, reasoning and finish reason.
#[tokio::test]
async fn a_repaired_turn_counts_the_calls_that_replaced_the_original() {
    let outcome = drain_reissued(200, two_good_calls()).await;

    assert!(outcome.repair_succeeded, "the re-issue replaced the call");
    assert_eq!(outcome.tool_calls, 2, "the two sent, not the one held");
    assert_eq!(outcome.completion_tokens, Some(37), "not the re-issue's 99");
    assert_eq!(outcome.reasoning_chars, 4, "not the re-issue's reasoning");
    assert_eq!(outcome.finish_reason.as_deref(), Some("tool_calls"));
}

/// A re-issue that fails leaves the original call to go out, and it is the
/// one counted.
#[tokio::test]
async fn a_failed_reissue_counts_the_original_call() {
    let outcome = drain_reissued(500, two_good_calls()).await;

    assert!(outcome.repair_attempted && !outcome.repair_succeeded);
    assert_eq!(outcome.tool_calls, 1);
    assert_eq!(outcome.completion_tokens, Some(37));
}
