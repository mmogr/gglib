//! A reply's rows rebuilt from its frames, which are the real events'
//! serialisation, so the parse here cannot drift from what the loop logs.

use serde_json::{Value, json};

use super::*;
use crate::domain::agent::AgentEvent;
use crate::domain::chat::MessageRole;

fn frames(events: &[AgentEvent]) -> Vec<String> {
    events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect()
}

fn rows(events: &[AgentEvent], finished: bool) -> Vec<NewMessage> {
    let frames = frames(events);
    rows_from_frames(frames.iter().map(String::as_str), finished, 9)
}

fn text(content: &str) -> AgentEvent {
    AgentEvent::TextDelta {
        content: content.to_owned(),
    }
}

fn call(id: &str) -> ToolCall {
    ToolCall {
        id: id.to_owned(),
        name: "read_file".to_owned(),
        arguments: json!({ "path": "a.rs" }),
    }
}

fn started(id: &str) -> AgentEvent {
    AgentEvent::ToolCallStart {
        tool_call: call(id),
        display_name: "Read File".to_owned(),
        args_summary: None,
    }
}

fn completed(id: &str, content: &str) -> AgentEvent {
    AgentEvent::ToolCallComplete {
        tool_name: "read_file".to_owned(),
        result: ToolResult {
            tool_call_id: id.to_owned(),
            content: content.to_owned(),
            success: true,
        },
        wait_ms: 0,
        execute_duration_ms: 1,
        display_name: "Read File".to_owned(),
        duration_display: "1ms".to_owned(),
    }
}

fn meta(row: &NewMessage, key: &str) -> Value {
    row.metadata
        .as_ref()
        .and_then(|m| m.get(key))
        .cloned()
        .unwrap_or(Value::Null)
}

/// A turn with reasoning and two tool calls, then a final answer: the page's
/// shape, with the results in the calls' order whatever order they came in.
#[test]
fn a_finished_reply_is_one_row_per_turn_and_one_per_result() {
    let got = rows(
        &[
            AgentEvent::PromptProgress {
                processed: 1,
                total: 2,
                cached: 0,
                time_ms: 3,
            },
            AgentEvent::ReasoningDelta {
                content: "think ".to_owned(),
            },
            AgentEvent::ReasoningDelta {
                content: "more".to_owned(),
            },
            text("Let me "),
            text("look."),
            started("c1"),
            started("c2"),
            completed("c2", "second"),
            completed("c1", "first"),
            AgentEvent::IterationComplete {
                iteration: 1,
                tool_calls: 2,
            },
            AgentEvent::SystemWarning {
                message: "retrying".to_owned(),
                suggested_action: None,
            },
            text("The answ"),
            AgentEvent::FinalAnswer {
                content: "The answer.".to_owned(),
            },
        ],
        true,
    );

    let roles: Vec<MessageRole> = got.iter().map(|r| r.role).collect();
    assert_eq!(
        roles,
        [
            MessageRole::Assistant,
            MessageRole::Tool,
            MessageRole::Tool,
            MessageRole::Assistant
        ]
    );
    assert!(got.iter().all(|r| r.conversation_id == 9));
    assert_eq!(got[0].content, "Let me look.");
    assert_eq!(meta(&got[0], THINKING_KEY), json!("think more"));
    let calls = meta(&got[0], "tool_calls");
    assert_eq!(calls[0]["id"], "c1");
    assert_eq!(calls[1]["id"], "c2");
    assert_eq!(calls[0]["arguments"], json!({ "path": "a.rs" }));
    assert_eq!(
        (got[1].content.as_str(), meta(&got[1], "tool_call_id")),
        ("first", json!("c1"))
    );
    assert_eq!(
        (got[2].content.as_str(), meta(&got[2], "tool_call_id")),
        ("second", json!("c2"))
    );
    assert_eq!(got[3].content, "The answer.");
    assert_eq!(got[3].metadata, None);
    assert!(got.iter().all(|r| meta(r, INCOMPLETE_KEY).is_null()));
}

/// Cancelled mid-tool-call: what arrived is kept, and the last assistant
/// row says the reply stopped.
#[test]
fn an_unfinished_reply_keeps_what_arrived_and_marks_the_last_assistant_row() {
    let got = rows(
        &[
            text("Checking."),
            started("c1"),
            started("c2"),
            completed("c1", "first"),
        ],
        false,
    );

    assert_eq!(got.len(), 2, "the assistant row and the one result");
    assert_eq!(got[0].role, MessageRole::Assistant);
    assert_eq!(got[0].content, "Checking.");
    assert_eq!(
        meta(&got[0], "tool_calls").as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(meta(&got[0], INCOMPLETE_KEY), json!(true));
    assert_eq!(got[1].role, MessageRole::Tool);
}

#[test]
fn an_unfinished_reply_after_a_whole_turn_marks_the_partial_answer() {
    let got = rows(
        &[
            started("c1"),
            completed("c1", "first"),
            AgentEvent::IterationComplete {
                iteration: 1,
                tool_calls: 1,
            },
            text("Half an"),
        ],
        false,
    );

    assert_eq!(got.len(), 3);
    assert!(meta(&got[0], INCOMPLETE_KEY).is_null());
    assert_eq!(got[2].content, "Half an");
    assert_eq!(meta(&got[2], INCOMPLETE_KEY), json!(true));
}

#[test]
fn a_reply_that_produced_nothing_still_says_it_stopped() {
    let got = rows(
        &[AgentEvent::Error {
            message: "LLM stream error".to_owned(),
        }],
        false,
    );

    assert_eq!(got.len(), 1);
    assert_eq!(got[0].role, MessageRole::Assistant);
    assert_eq!(got[0].content, "");
    assert_eq!(meta(&got[0], INCOMPLETE_KEY), json!(true));
}

#[test]
fn a_finished_reply_with_nothing_logged_writes_nothing_and_bad_frames_are_skipped() {
    assert!(rows_from_frames(["not json", "{\"type\":\"new_kind\"}"], true, 9).is_empty());
}
