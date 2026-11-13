//! The turns the generation tests drain: llama-server's frames for chosen
//! events, each sent after a wait of its own, read by a client that stays or
//! leaves.

use std::time::Duration;

use futures_util::stream::{self, Stream};

use super::*;
use crate::generation::TurnGeneration;
use crate::llama_frame_fixtures::llama_bytes;
use crate::upstream_read::{STREAM_IDLE_TIMEOUT, upstream_frames};

pub(super) fn reasoning(content: &str) -> LlmStreamEvent {
    LlmStreamEvent::ReasoningDelta {
        content: content.to_owned(),
    }
}

pub(super) fn text(content: &str) -> LlmStreamEvent {
    LlmStreamEvent::TextDelta {
        content: content.to_owned(),
    }
}

/// The first fragment of the call at `index`: its id and name, no arguments.
pub(super) fn call_opens(index: usize, name: &str) -> LlmStreamEvent {
    LlmStreamEvent::ToolCallDelta {
        index,
        id: Some(format!("call_{index}")),
        name: Some(name.to_owned()),
        arguments: None,
    }
}

/// A later fragment of the call at `index`: arguments only.
pub(super) fn call_continues(index: usize, arguments: &str) -> LlmStreamEvent {
    LlmStreamEvent::ToolCallDelta {
        index,
        id: None,
        name: None,
        arguments: Some(arguments.to_owned()),
    }
}

pub(super) fn done(finish_reason: &str) -> LlmStreamEvent {
    LlmStreamEvent::Done {
        finish_reason: Some(finish_reason.to_owned()),
    }
}

/// A usage frame whose three counts differ, so one read for another shows.
pub(super) fn usage(completion_tokens: u32) -> LlmStreamEvent {
    LlmStreamEvent::Usage {
        prompt_tokens: 11,
        completion_tokens,
        total_tokens: 11 + completion_tokens,
        cached_tokens: None,
    }
}

/// An upstream that sends the frame llama-server writes for each event once
/// its wait, in milliseconds, is over.
pub(super) fn upstream(
    events: Vec<(u64, LlmStreamEvent)>,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static {
    stream::iter(events).then(|(wait, event)| async move {
        tokio::time::sleep(Duration::from_millis(wait)).await;
        Ok(llama_bytes(&event))
    })
}

/// A prefill progress frame, which is not a generated token.
pub(super) fn prefill() -> LlmStreamEvent {
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
pub(super) fn full_turn() -> Vec<(u64, LlmStreamEvent)> {
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
pub(super) fn full_turn_generation() -> TurnGeneration {
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
pub(super) fn holding() -> RepairContext {
    RepairContext {
        req_builder: crate::loopback::client().post("http://127.0.0.1:9/"),
        request_body: Bytes::new(),
        enabled: false,
    }
}

/// How the client reads the turn.
#[derive(Clone, Copy)]
pub(super) enum Client {
    /// It takes every frame.
    Stays,
    /// It takes this many frames, waits this many milliseconds, and hangs up.
    /// With no wait it hangs up before the drain runs again.
    Leaves { after: usize, wait: u64 },
}

/// Drain `events` to a client that reads as `client` says, through a
/// one-frame channel: a second frame is not sent until the first is taken.
pub(super) async fn drain(
    events: Vec<(u64, LlmStreamEvent)>,
    repair: Option<RepairContext>,
    client: Client,
) -> StreamOutcome {
    drain_stream(upstream(events), repair, client).await
}

/// [`drain`] over any upstream, so one that breaks can be drained too.
pub(super) async fn drain_stream(
    bytes: impl Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
    repair: Option<RepairContext>,
    client: Client,
) -> StreamOutcome {
    let registry = Arc::new(crate::connections::ActiveConnectionsRegistry::new());
    let connection = registry.register(1, "m", true, None);
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
        upstream_frames(bytes, STREAM_IDLE_TIMEOUT),
        "m".to_owned(),
        ClientSender::new(tx, Duration::from_secs(5)),
        &connection,
        repair,
        None,
    )
    .await;
    reader.await.expect("the client task does not panic");
    outcome
}
