//! Writing time on a paced stream, through the real normaliser.
//!
//! The clock is injected and each raw event advances it before it arrives,
//! so the stream is paced exactly: a prefill, then one token every 40 ms.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::StreamExt as _;
use gglib_core::domain::DialectSpec;
use gglib_core::domain::agent::LlmStreamEvent;

use super::super::stream::{EventStream, normalize_timed};
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

/// A turn that only calls a tool, in qwen-xml markup: 24 tokens, the first
/// after the prefill, then one every 40 ms; then its end and its usage.
fn tool_call_turn() -> Vec<(u64, LlmStreamEvent)> {
    let tokens = [
        "<tool_call>",
        "\n",
        "{\"",
        "name",
        "\":",
        " \"",
        "read",
        "_file",
        "\",",
        " \"",
        "arguments",
        "\":",
        " {\"",
        "path",
        "\":",
        " \"",
        "a",
        ".",
        "rs",
        "\"}",
        "}",
        "\n",
        "</tool_call>",
        "",
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
    // The last token is the empty delta a llama-server ends with: no text,
    // so not counted as written.
    for (i, token) in tokens.iter().enumerate() {
        let after = if i == 0 { PREFILL_MS } else { STEP_MS };
        events.push((after, text(token)));
    }
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

async fn normalized(events: Vec<(u64, LlmStreamEvent)>) -> Vec<LlmStreamEvent> {
    let now = Arc::new(AtomicU64::new(0));
    let dialect = DialectSpec::qwen_xml();
    let stream = normalize_timed(paced(events, &now), Some(&dialect), clock(&now));
    stream.map(|item| item.expect("no error")).collect().await
}

/// The parser releases the whole call as one delta at the close tag, yet
/// the writing time is the 22 steps between the first token and the last:
/// the rate it gives is the model's, not one decode step's.
#[tokio::test]
async fn a_held_back_tool_call_is_timed_from_its_first_token_to_its_last() {
    let events = normalized(tool_call_turn()).await;
    let calls = events
        .iter()
        .filter(|e| matches!(e, LlmStreamEvent::ToolCallDelta { name: Some(_), .. }))
        .count();
    assert_eq!(
        calls, 1,
        "the markup was held back and released as one call"
    );

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
    let events = normalized(vec![
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
