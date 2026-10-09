//! The rows an agent run saves for one turn, pinned whole: every row, in
//! order, with its content and its metadata.
//!
//! `transcript_turn.json`, beside the writer in `gglib-app-services`, holds
//! one turn as the events its loop sends and the rows it is saved as, when
//! it finishes and when it stops before its answer. The CLI's chat is held
//! to the same file by its own test, so the two surfaces save a turn alike.

use gglib_core::domain::agent::{AgentEvent, ContextReading, ToolCall, ToolResult, TurnUsage};
use gglib_core::domain::chat::Message;
use serde_json::{Value, json};

use super::run_fixture::{End, conversation, prepared, saved, settled, start, state};

const TURN: &str = include_str!("../../../../gglib-app-services/src/transcript_turn.json");

fn usage(prompt: u32, written: u32, stopped: &str, trimmed: usize) -> AgentEvent {
    AgentEvent::TurnUsage(TurnUsage {
        prompt_tokens: Some(prompt),
        cached_tokens: Some(10),
        completion_tokens: Some(written),
        duration_ms: 700,
        writing_ms: Some(500),
        finish_reason: Some(stopped.to_owned()),
        reading: ContextReading::new(None, trimmed),
        ..TurnUsage::default()
    })
}

/// A turn that reasons, calls a tool and answers, in the order the loop
/// sends it: a model call's usage once its stream ends, then its tools.
fn turn() -> Vec<AgentEvent> {
    let text = |content: &str| AgentEvent::TextDelta {
        content: content.to_owned(),
    };
    vec![
        AgentEvent::ReasoningDelta {
            content: "The file will say.".to_owned(),
        },
        text("Reading "),
        text("the file."),
        usage(40, 12, "tool_calls", 2),
        AgentEvent::ToolCallStart {
            tool_call: ToolCall {
                id: "c1".to_owned(),
                name: "read_file".to_owned(),
                arguments: json!({ "path": "src/main.rs" }),
            },
            display_name: "Read File".to_owned(),
            args_summary: Some("src/main.rs".to_owned()),
        },
        AgentEvent::ToolCallComplete {
            tool_name: "read_file".to_owned(),
            result: ToolResult::text("c1".to_owned(), "fn main() {}".to_owned(), true),
            wait_ms: 0,
            execute_duration_ms: 1,
            display_name: "Read File".to_owned(),
            duration_display: "1ms".to_owned(),
        },
        AgentEvent::IterationComplete {
            iteration: 1,
            tool_calls: 1,
        },
        text("It is an empty main."),
        usage(64, 7, "stop", 0),
        AgentEvent::FinalAnswer {
            content: "It is an empty `main`.".to_owned(),
        },
    ]
}

/// A saved row as the file spells one: all of it but its ids and its time.
fn spelled(row: &Message) -> Value {
    json!({
        "role": row.role,
        "content": row.content,
        "metadata": row.metadata,
        "images": row.images.len(),
    })
}

fn events_of(turn: &[AgentEvent]) -> Value {
    turn.iter()
        .map(|event| serde_json::to_value(event).unwrap())
        .collect()
}

/// What a run of `events` that ends as `end` saves to a conversation.
async fn rows_saved(events: Vec<AgentEvent>, end: End) -> Value {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (p, _) = prepared(events, end);
    start(&state, "a1", Some(id), p).await;
    settled(&state).await;
    saved(&state, id).await.iter().map(spelled).collect()
}

#[tokio::test]
async fn a_finished_turn_is_saved_as_the_rows_the_file_holds() {
    let pinned: Value = serde_json::from_str(TURN).unwrap();
    assert_eq!(events_of(&turn()), pinned["events"], "the file's own turn");

    let rows = rows_saved(turn(), End::Finish).await;

    assert_eq!(rows, pinned["rows"], "{rows:#}");
}

/// The same turn stopped before its answer: what arrived is saved, and its
/// last assistant row says the reply did not finish.
#[tokio::test]
async fn a_turn_that_stops_before_its_answer_is_saved_as_the_rows_the_file_holds() {
    let pinned: Value = serde_json::from_str(TURN).unwrap();
    let mut stopped = turn();
    stopped.pop();

    let rows = rows_saved(stopped, End::Fail).await;

    assert_eq!(rows, pinned["rows_when_stopped"], "{rows:#}");
}
