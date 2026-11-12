//! Writing time on a paced stream.
//!
//! The clock is injected and each raw event advances it before it arrives,
//! so the stream is paced exactly: a prefill, then one token every 40 ms.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::StreamExt as _;
use gglib_core::domain::agent::LlmStreamEvent;

use super::super::stream::EventStream;
use super::*;

const PREFILL_MS: u64 = 500;
const STEP_MS: u64 = 40;

/// `events`, each arriving `after` ms of the shared clock after the last.
fn paced(events: Vec<(u64, LlmStreamEvent)>, now: &Arc<AtomicU64>) -> EventStream {
    let now = Arc::clone(now);
    Box::pin(
        futures_util::stream::iter(events).map(move |(after, event)| {
            now.fetch_add(after, Ordering::SeqCst);
            Ok(event)
        }),
    )
}

fn clock(now: &Arc<AtomicU64>) -> Clock {
    let now = Arc::clone(now);
    Box::new(move || now.load(Ordering::SeqCst))
}

fn text(content: &str) -> LlmStreamEvent {
    LlmStreamEvent::TextDelta {
        content: content.to_owned(),
    }
}

/// A turn that only calls a tool, as llama-server streams one it parsed:
/// the call's name, then 22 argument tokens, the first after the prefill,
/// then one every 40 ms; then an empty delta, its end and its usage.
fn tool_call_turn() -> Vec<(u64, LlmStreamEvent)> {
    let arguments = [
        "{\"", "path", "\":", " \"", "src", "/", "lib", ".", "rs", "\",", " \"", "max", "_lines",
        "\":", " ", "4", "2", ",", " \"", "x", "\":", "1}",
    ];
    let mut events = vec![(
        0,
        LlmStreamEvent::PromptProgress {
            processed: 10,
            total: 10,
            cached: 0,
            time_ms: 0,
        },
    )];
    events.push((
        PREFILL_MS,
        LlmStreamEvent::ToolCallDelta {
            index: 0,
            id: Some("call_1".to_owned()),
            name: Some("read_file".to_owned()),
            arguments: Some(String::new()),
        },
    ));
    for token in arguments {
        events.push((
            STEP_MS,
            LlmStreamEvent::ToolCallDelta {
                index: 0,
                id: None,
                name: None,
                arguments: Some(token.to_owned()),
            },
        ));
    }
    // The empty delta a llama-server ends with: no text, so not counted as
    // written.
    events.push((STEP_MS, text("")));
    events.push((
        1,
        LlmStreamEvent::Done {
            finish_reason: Some("tool_calls".to_owned()),
        },
    ));
    events.push((
        1,
        LlmStreamEvent::Usage {
            prompt_tokens: 10,
            completion_tokens: 23,
            total_tokens: 33,
            cached_tokens: None,
        },
    ));
    events
}

async fn timed(events: Vec<(u64, LlmStreamEvent)>) -> Vec<LlmStreamEvent> {
    let now = Arc::new(AtomicU64::new(0));
    let stream = time_writing(paced(events, &now), clock(&now));
    stream.map(|item| item.expect("no error")).collect().await
}

/// A turn that only calls a tool is timed over its 22 steps, from the first
/// token to the last: the rate it gives is the model's.
#[tokio::test]
async fn a_tool_call_is_timed_from_its_first_token_to_its_last() {
    let events = timed(tool_call_turn()).await;

    let writing: Vec<u64> = events
        .iter()
        .filter_map(|e| match e {
            LlmStreamEvent::WritingTime { ms } => Some(*ms),
            _ => None,
        })
        .collect();
    assert_eq!(
        writing,
        [22 * STEP_MS],
        "first token to last, the prefill excluded"
    );

    let ms = |v: u64| f64::from(u32::try_from(v).unwrap());
    let rate = 23.0 * 1000.0 / ms(writing[0]);
    let true_rate = 1000.0 / ms(STEP_MS);
    assert!(
        (rate - true_rate).abs() / true_rate < 0.1,
        "the rate shown is {rate:.1} tok/s"
    );
}

#[tokio::test]
async fn a_single_token_gives_no_writing_time_rather_than_a_guess() {
    let events = timed(vec![
        (PREFILL_MS, text("Yes")),
        (
            1,
            LlmStreamEvent::Done {
                finish_reason: None,
            },
        ),
    ])
    .await;
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, LlmStreamEvent::WritingTime { .. }))
    );
}
