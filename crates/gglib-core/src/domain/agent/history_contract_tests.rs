//! The history a turn starts from, as the daemon rebuilds it from a chat's
//! saved rows, written to `contracts/chats/history.json` for the page.
//!
//! Before the page sent its turns as `{conversation_id, content}`, it sent
//! the whole history, rebuilt from the rows it had loaded
//! (`buildThreadMessages`, then `convertToWireMessages`). The page's test
//! replays these rows through that and must get [`saved_history`]'s
//! messages: the daemon's history is the one that remains.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::*;
use crate::domain::agent::{
    AgentEvent, ToolCall, ToolResult, UNFINISHED_TOOL_CALL, rows_from_frames,
};
use crate::domain::attachment::{AttachmentId, AttachmentInfo};

const CONVERSATION: i64 = 7;

fn image(seed: &str) -> AttachmentInfo {
    AttachmentInfo {
        id: AttachmentId::of(seed.as_bytes()),
        mime: "image/png".to_owned(),
        width: 640,
        height: 480,
    }
}

/// Saved rows, numbered from 1 in order, as `GET /api/conversations/{id}/thread`
/// gives them. Images are looked up by id among `images`.
fn saved(rows: Vec<NewMessage>, images: &[AttachmentInfo]) -> Vec<Message> {
    rows.into_iter()
        .zip(1..)
        .map(|(row, id)| Message {
            id,
            conversation_id: row.conversation_id,
            role: row.role,
            content: row.content,
            created_at: format!("2026-11-08 09:00:{id:02}"),
            metadata: row.metadata,
            images: row
                .images
                .iter()
                .map(|id| images.iter().find(|i| &i.id == id).unwrap().clone())
                .collect(),
            origin_id: None,
        })
        .collect()
}

fn user(content: &str, images: &[AttachmentInfo]) -> NewMessage {
    let message = AgentMessage::User {
        content: content.to_owned(),
        images: images.iter().map(|i| i.id.clone()).collect(),
    };
    to_new_message(&message, CONVERSATION)
}

fn plain(role: MessageRole, content: &str) -> NewMessage {
    NewMessage {
        conversation_id: CONVERSATION,
        role,
        content: content.to_owned(),
        metadata: None,
        images: Vec::new(),
    }
}

/// The rows a reply's logged events are saved as, the way a run's end saves
/// them.
fn reply(events: &[AgentEvent], finished: bool) -> Vec<NewMessage> {
    let frames: Vec<String> = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect();
    rows_from_frames(frames.iter().map(String::as_str), finished, CONVERSATION)
}

fn text(content: &str) -> AgentEvent {
    AgentEvent::TextDelta {
        content: content.to_owned(),
    }
}

fn reasoning(content: &str) -> AgentEvent {
    AgentEvent::ReasoningDelta {
        content: content.to_owned(),
    }
}

fn answer(content: &str) -> AgentEvent {
    AgentEvent::FinalAnswer {
        content: content.to_owned(),
    }
}

fn call(id: &str, name: &str, arguments: Value, server: Option<&str>) -> AgentEvent {
    AgentEvent::ToolCallStart {
        tool_call: ToolCall {
            id: id.to_owned(),
            name: name.to_owned(),
            arguments,
        },
        display_name: name.to_owned(),
        args_summary: None,
        server: server.map(str::to_owned),
    }
}

fn done(id: &str, name: &str, content: &str, images: Vec<AttachmentInfo>) -> AgentEvent {
    AgentEvent::ToolCallComplete {
        tool_name: name.to_owned(),
        result: ToolResult {
            tool_call_id: id.to_owned(),
            content: content.to_owned(),
            success: true,
            images,
        },
        wait_ms: 0,
        execute_duration_ms: 3,
        display_name: name.to_owned(),
        duration_display: "3ms".to_owned(),
    }
}

fn iteration(n: usize, calls: usize) -> AgentEvent {
    AgentEvent::IterationComplete {
        iteration: n,
        tool_calls: calls,
    }
}

/// One case: a chat's prompt and rows, and the history its next turn starts
/// from.
fn case(name: &str, prompt: Option<&str>, rows: &[Message]) -> Value {
    json!({
        "name": name,
        "system_prompt": prompt,
        "rows": rows,
        "history": saved_history(prompt, rows),
    })
}

fn history_contract() -> Value {
    let photo = image("photo");
    let chart = image("chart");
    let drawn = image("drawn");
    let images = [photo.clone(), chart.clone(), drawn.clone()];
    let rows = |rows: Vec<Vec<NewMessage>>| saved(rows.concat(), &images);

    let answered = rows(vec![
        vec![user("What is 2 + 2?", &[])],
        reply(&[reasoning("Add them."), text("4"), answer("4")], true),
    ]);
    let tools = rows(vec![
        vec![user("What does a.txt say?", &[])],
        reply(
            &[
                text("Let me look."),
                call("call_1", "read_file", json!({ "path": "a.txt" }), None),
                call("call_2", "1:search", json!({ "q": "hi" }), Some("web")),
                done("call_1", "read_file", "hi", vec![]),
                done("call_2", "1:search", "{\"hits\":1}", vec![]),
                iteration(1, 2),
                call("call_3", "builtin:generate_image", json!({}), None),
                done("call_3", "builtin:generate_image", "drawn", vec![drawn]),
                iteration(2, 1),
                answer("It says hi."),
            ],
            true,
        ),
    ]);
    let unfinished = rows(vec![
        vec![user("Read b.txt", &[])],
        reply(
            &[
                text("Reading"),
                call("call_9", "read_file", json!({ "path": "b.txt" }), None),
            ],
            false,
        ),
    ]);
    let stopped = rows(vec![vec![user("Hello?", &[])], reply(&[], false)]);
    let pictures = rows(vec![
        vec![user("What is in these?", &[photo.clone(), chart])],
        reply(&[answer("A cat and a chart.")], true),
        vec![user("", &[photo])],
        reply(&[answer("The cat again.")], true),
    ]);
    // An edit, a regenerate or a branch leaves the chat ending in a question.
    let asked = rows(vec![
        vec![user("First", &[])],
        reply(&[answer("One")], true),
        vec![user("Second, edited", &[])],
    ]);
    let system_row = rows(vec![
        vec![plain(MessageRole::System, "An old prompt kept as a row.")],
        vec![user("Hi", &[])],
        reply(&[answer("Hello.")], true),
    ]);

    json!({ "cases": [
        case("a prompt, a question and its reasoned answer", Some("  Be brief.\n"), &answered),
        case("no prompt", None, &answered),
        case("a blank prompt", Some("   "), &answered),
        case("tool calls, folded on the page, and a drawn picture", Some("You help."), &tools),
        case("an unfinished reply, stopped mid-call", Some("You help."), &unfinished),
        case("a reply stopped before it said anything", None, &stopped),
        case("images on the user's messages, one with no words", None, &pictures),
        case("a chat that ends in its question", Some("You help."), &asked),
        case("a system row is never sent", Some("You help."), &system_row),
    ]})
}

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/chats/history.json")
}

/// The page replays these rows and must send what the daemon rebuilds.
/// `GGLIB_RECORD_CONTRACTS=1` rewrites the file after a deliberate change.
#[test]
fn the_history_contract_is_current() {
    let want = serde_json::to_string_pretty(&history_contract()).unwrap() + "\n";
    let path = contract_path();
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::write(&path, &want).expect("write history.json");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/chats/history.json");
    assert_eq!(
        have, want,
        "contracts/chats/history.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1"
    );
}

/// An unfinished call is answered in the history by the sentence the saved
/// row holds, so the model is never sent a call without its result.
#[test]
fn an_unfinished_call_is_sent_with_the_sentence_its_row_holds() {
    let contract = history_contract();
    let unfinished = &contract["cases"][4]["history"];
    let last = unfinished.as_array().unwrap().last().unwrap();
    assert_eq!(
        last,
        &json!({ "role": "tool", "tool_call_id": "call_9", "content": UNFINISHED_TOOL_CALL })
    );
}
