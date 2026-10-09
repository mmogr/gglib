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
        result: ToolResult::text(id.to_owned(), content.to_owned(), true),
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

    assert_eq!(got.len(), 3, "the assistant row and a row per call");
    assert_eq!(got[0].role, MessageRole::Assistant);
    assert_eq!(got[0].content, "Checking.");
    assert_eq!(
        meta(&got[0], "tool_calls").as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(meta(&got[0], INCOMPLETE_KEY), json!(true));
    assert_eq!(
        (got[1].content.as_str(), meta(&got[1], "tool_call_id")),
        ("first", json!("c1"))
    );
    assert_eq!(
        (got[2].content.as_str(), meta(&got[2], "tool_call_id")),
        (UNFINISHED_TOOL_CALL, json!("c2")),
        "a call with no answer still gets its row"
    );
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

fn reasoning(content: &str) -> AgentEvent {
    AgentEvent::ReasoningDelta {
        content: content.to_owned(),
    }
}

/// How long each turn thought, from its first reasoning event's time to its
/// last's, in tenths of a second; a turn that did not reason records none.
#[test]
fn each_turn_records_how_long_it_thought() {
    let events = [
        reasoning("a"),
        reasoning("b"),
        text("t"),
        AgentEvent::IterationComplete {
            iteration: 1,
            tool_calls: 0,
        },
        text("no thinking"),
        AgentEvent::IterationComplete {
            iteration: 2,
            tool_calls: 0,
        },
        reasoning("c"),
        reasoning("d"),
        AgentEvent::FinalAnswer {
            content: "done".to_owned(),
        },
    ];
    let logged_at = [
        1_000, 3_560, 3_600, 3_700, 4_000, 4_100, 5_000, 5_049, 6_000,
    ];
    let frames = frames(&events);
    let with_times = frames.iter().map(String::as_str).zip(logged_at.map(Some));

    let rows = rows_from_timed_frames(with_times, true, 9);

    let seconds: Vec<Value> = rows
        .iter()
        .map(|r| {
            r.metadata
                .as_ref()
                .and_then(|m| m.get(THINKING_DURATION_KEY))
                .cloned()
                .unwrap_or(Value::Null)
        })
        .collect();
    assert_eq!(seconds, [json!(2.5), Value::Null, json!(0.0)]);
}

#[test]
fn frames_with_no_times_record_no_duration() {
    let rows = rows(&[reasoning("a"), text("t")], true);
    let meta = rows[0].metadata.as_ref().unwrap();
    assert_eq!(meta[THINKING_KEY], json!("a"));
    assert!(meta.get(THINKING_DURATION_KEY).is_none());
}

/// A result's images go on its tool row, by id and in order; the row's text
/// is the result's text alone, and no other row gains an image.
#[test]
fn a_tool_row_carries_the_ids_of_the_images_its_result_made() {
    let image = |bytes: &[u8]| crate::domain::AttachmentInfo {
        id: crate::domain::AttachmentId::of(bytes),
        mime: "image/png".to_owned(),
        width: 1,
        height: 1,
    };
    let mut drawn = ToolResult::text("c1", "[image 1x1 PNG stored]", true);
    drawn.images = vec![image(b"one"), image(b"two")];
    let events = [
        started("c1"),
        started("c2"),
        AgentEvent::ToolCallComplete {
            tool_name: "draw".to_owned(),
            result: drawn,
            wait_ms: 0,
            execute_duration_ms: 1,
            display_name: "Draw".to_owned(),
            duration_display: "1ms".to_owned(),
        },
        completed("c2", "plain"),
        AgentEvent::IterationComplete {
            iteration: 1,
            tool_calls: 2,
        },
        AgentEvent::FinalAnswer {
            content: "done".to_owned(),
        },
    ];
    let frames = frames(&events);

    let got = rows_from_timed_frames(frames.iter().map(|f| (f.as_str(), Some(0))), true, 9);

    assert_eq!(got.len(), 4, "an assistant row, two tool rows, the answer");
    assert_eq!(got[1].role, MessageRole::Tool);
    assert_eq!(got[1].content, "[image 1x1 PNG stored]");
    assert_eq!(
        got[1].images,
        [
            crate::domain::AttachmentId::of(b"one"),
            crate::domain::AttachmentId::of(b"two")
        ]
    );
    assert!(got[2].images.is_empty(), "a result without images");
    assert!(got[0].images.is_empty() && got[3].images.is_empty());
}
