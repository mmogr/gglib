//! Who is told a reading, and every byte the client that did not ask is
//! sent.

use std::sync::Arc;

use gglib_core::LlmStreamEvent;
use gglib_core::domain::agent::ContextReading;
use gglib_core::request_pipeline::TruncationReport;
use gglib_core::sse::DONE_SENTINEL;
use serde_json::{Value, json};

use super::{Told, told};
use crate::client_send::{CLIENT_SEND_TIMEOUT, ClientSender};
use crate::connections::ActiveConnectionsRegistry;
use crate::forward::drain_events;
use crate::llama_frame_fixtures::{llama_bytes, llama_frame};
use crate::upstream_read::{STREAM_IDLE_TIMEOUT, prefill_comment, upstream_frames};

fn shortened(messages_truncated: usize) -> TruncationReport {
    TruncationReport {
        messages_truncated,
        ..TruncationReport::default()
    }
}

#[test]
fn a_client_that_asked_is_told_the_launched_context_and_what_was_shortened() {
    assert_eq!(
        told(true, 8192, &shortened(3)),
        Some(ContextReading {
            context_size: Some(8192),
            trimmed_messages: Some(3),
        })
    );
}

#[test]
fn a_client_that_did_not_ask_is_told_nothing() {
    assert_eq!(told(false, 8192, &shortened(3)), None);
}

/// Nothing shortened is a count left out, with the context still told.
#[test]
fn nothing_shortened_tells_the_context_alone() {
    assert_eq!(
        told(true, 4096, &shortened(0)),
        Some(ContextReading {
            context_size: Some(4096),
            trimmed_messages: None,
        })
    );
}

fn progress() -> LlmStreamEvent {
    LlmStreamEvent::PromptProgress {
        processed: 57,
        total: 57,
        cached: 0,
        time_ms: 813,
    }
}

fn text() -> LlmStreamEvent {
    LlmStreamEvent::TextDelta {
        content: "Moonlight".to_owned(),
    }
}

fn done() -> LlmStreamEvent {
    LlmStreamEvent::Done {
        finish_reason: Some("stop".to_owned()),
    }
}

fn usage() -> LlmStreamEvent {
    LlmStreamEvent::Usage {
        prompt_tokens: 812,
        completion_tokens: 96,
        total_tokens: 908,
        cached_tokens: Some(700),
    }
}

/// Every byte one client is sent for a turn that prefilled, answered,
/// finished and reported its usage.
async fn wire(told: Told) -> String {
    let registry = Arc::new(ActiveConnectionsRegistry::new());
    let connection = registry.register(1, "m", true, None);
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let bytes = [progress(), text(), done(), usage()].map(|e| Ok(llama_bytes(&e)));
    drain_events(
        upstream_frames::<_, std::io::Error>(
            futures_util::stream::iter(bytes),
            STREAM_IDLE_TIMEOUT,
        ),
        "m".to_owned(),
        ClientSender::new(tx, CLIENT_SEND_TIMEOUT),
        &connection,
        None,
        told,
    )
    .await;
    let mut wire = String::new();
    while let Some(Ok(chunk)) = rx.recv().await {
        wire.push_str(&String::from_utf8_lossy(&chunk));
    }
    wire
}

/// The usage frame of `wire`, parsed.
fn usage_frame(wire: &str) -> Value {
    let mut frames = wire
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .filter(|frame| frame.get("usage").is_some());
    let frame = frames.next().expect("a usage frame");
    assert!(frames.next().is_none(), "one usage frame");
    frame
}

/// A client whose body did not set `return_progress` is sent a comment for
/// the prefill, then llama-server's frames with only `model` changed, the
/// upstream's counts alone in `usage`, and the one `[DONE]`.
#[tokio::test]
async fn a_client_that_did_not_ask_gets_the_frame_unchanged() {
    let wire = wire(None).await;

    let blocks: Vec<&str> = wire.split_inclusive("\n\n").collect();
    let [comment, frames @ .., sentinel] = blocks.as_slice() else {
        panic!("a comment, frames and [DONE]: {wire}");
    };
    assert_eq!(comment.as_bytes(), &prefill_comment(57, 57)[..]);
    assert_eq!(*sentinel, DONE_SENTINEL);
    let sent: Vec<Value> = frames
        .iter()
        .map(|block| {
            let payload = block.strip_prefix("data: ").expect("a data block");
            serde_json::from_str(payload.trim_end()).expect("JSON")
        })
        .collect();
    let want: Vec<Value> = [text(), done(), usage()]
        .iter()
        .map(|event| {
            let mut frame = llama_frame(event);
            frame["model"] = json!("m");
            frame
        })
        .collect();
    assert_eq!(sent, want);
    assert_eq!(
        usage_frame(&wire)["usage"],
        json!({
            "prompt_tokens": 812,
            "completion_tokens": 96,
            "total_tokens": 908,
            "prompt_tokens_details": { "cached_tokens": 700 },
        })
    );
}

/// A client that asked is told the reading inside `usage`, once, and its
/// other frames are those of a client told nothing new.
#[tokio::test]
async fn a_client_that_asked_gets_the_reading_in_its_usage_frame_alone() {
    let wire = wire(Some(ContextReading::new(Some(8192), 3))).await;

    let sent = usage_frame(&wire);
    assert_eq!(
        sent["usage"],
        json!({
            "prompt_tokens": 812,
            "completion_tokens": 96,
            "total_tokens": 908,
            "prompt_tokens_details": { "cached_tokens": 700 },
            "context_size": 8192,
            "trimmed_messages": 3,
        })
    );
    assert_eq!(sent["choices"], json!([]));
    assert_eq!(wire.matches("context_size").count(), 1, "{wire}");
    assert_eq!(wire.matches("trimmed_messages").count(), 1, "{wire}");
    // It asked for progress, so it gets the frame and no comment.
    assert_eq!(wire.matches("\"prompt_progress\"").count(), 1, "{wire}");
    assert!(!wire.contains(": prefill"), "{wire}");
}

/// A client that asked, of a reply whose context is not known and from
/// which nothing was shortened, is sent no key at all: the usage frame is
/// the one a client that did not ask gets.
#[tokio::test]
async fn a_reading_with_nothing_known_adds_no_key() {
    let asked = usage_frame(&wire(Some(ContextReading::default())).await);
    let unasked = usage_frame(&wire(None).await);
    assert_eq!(asked["usage"], unasked["usage"]);
}
