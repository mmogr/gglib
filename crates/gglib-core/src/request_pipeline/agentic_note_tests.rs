//! Tests for [`super`]: when a request is given the note, and where.

use serde_json::{Value, json};

use super::super::drawn_note::{DRAW_TOOL_NAME, DRAWN_NOTE};
use super::super::messages::shape_messages_noting;
use super::super::{DISABLE_AGENTIC_SAMPLING_ENV, SamplingLayers, apply, shape_messages};
use super::*;
use crate::domain::ModelCapabilities;

fn said(role: &str, text: &str) -> Value {
    json!({ "role": role, "content": text })
}

/// A request that offers one tool.
fn with_tools(messages: Vec<Value>) -> Value {
    json!({
        "model": "m",
        "messages": Value::Array(messages),
        "tools": [{ "type": "function", "function": { "name": "read_file" } }],
    })
}

fn unknown() -> ModelContext {
    ModelContext::passthrough()
}

/// The layers of the one caller that turns the note on.
fn on() -> SamplingLayers {
    SamplingLayers {
        agentic_prompt_note: true,
        ..SamplingLayers::default()
    }
}

/// How many times the note appears anywhere in `body`.
fn notes(body: &Value) -> usize {
    let escaped = Value::from(AGENTIC_NOTE).to_string();
    let escaped = escaped.trim_matches('"');
    body.to_string().matches(escaped).count()
}

/// `body` is left byte for byte as it was, by the stage and by the pipeline's
/// message shaping with the note on.
fn assert_untouched(body: &Value, ctx: &ModelContext) {
    let before = body.to_string();
    let mut shaped = body.clone();
    assert!(!note_batching(&mut shaped, ctx), "{before}");
    assert_eq!(shaped.to_string(), before);
    assert!(!shape_messages_noting(&mut shaped, ctx, true), "{before}");
    assert_eq!(shaped.to_string(), before);
}

// ── The text ──────────────────────────────────────────────────────────────

/// Four lines of plain ASCII, sixty words at most.
#[test]
fn the_note_is_four_short_lines_of_plain_text() {
    assert!(AGENTIC_NOTE.is_ascii());
    assert_eq!(AGENTIC_NOTE.lines().count(), 4);
    assert!(AGENTIC_NOTE.lines().all(|line| line.trim() == line));
    assert!(!AGENTIC_NOTE.contains('#'));
    assert!(AGENTIC_NOTE.split_whitespace().count() <= 60);
}

// ── Where ─────────────────────────────────────────────────────────────────

/// A system prompt that is a string gains the note after a blank line, and
/// every byte before it is the client's.
#[test]
fn the_note_follows_the_clients_system_prompt_after_a_blank_line() {
    let messages = vec![said("system", "Be brief."), said("user", "fix the bug")];
    let mut shaped = with_tools(messages.clone());
    let sent = shaped["messages"][0]["content"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(note_batching(&mut shaped, &unknown()));

    let prompt = shaped["messages"][0]["content"].as_str().unwrap();
    assert_eq!(prompt, format!("Be brief.\n\n{AGENTIC_NOTE}"));
    assert!(prompt.starts_with(&sent));
    assert_eq!(shaped["messages"][1], messages[1]);
    assert_eq!(shaped["tools"], with_tools(messages)["tools"]);
    assert_eq!(notes(&shaped), 1);
}

/// A system prompt sent as parts gains one more text part, and the parts the
/// client sent are unchanged.
#[test]
fn a_system_prompt_in_parts_gains_the_note_as_its_last_part() {
    let part = json!({ "type": "text", "text": "Be brief.", "cache_control": { "type": "x" } });
    let system = json!({ "role": "system", "content": [part] });
    let mut shaped = with_tools(vec![system, said("user", "fix the bug")]);
    assert!(note_batching(&mut shaped, &unknown()));

    assert_eq!(
        shaped["messages"][0]["content"],
        json!([part, { "type": "text", "text": AGENTIC_NOTE }])
    );
    assert_eq!(shaped["messages"].as_array().unwrap().len(), 2);
}

/// With no system message the note is one, before everything the client
/// sent, and what the client sent follows unchanged.
#[test]
fn a_request_with_no_system_message_gains_one_at_the_front() {
    let messages = vec![said("user", "fix the bug"), said("assistant", "ok")];
    let mut shaped = with_tools(messages.clone());
    assert!(note_batching(&mut shaped, &unknown()));

    let mut expected = vec![said("system", AGENTIC_NOTE)];
    expected.extend(messages);
    assert_eq!(shaped.to_string(), with_tools(expected).to_string());
}

/// A system message with no content to add text to is left alone, and the
/// note goes in a message of its own.
#[test]
fn a_system_message_with_no_text_content_is_left_and_the_note_goes_first() {
    let empty = json!({ "role": "system", "content": null });
    let mut shaped = with_tools(vec![empty.clone(), said("user", "go")]);
    assert!(note_batching(&mut shaped, &unknown()));

    assert_eq!(shaped["messages"][0], said("system", AGENTIC_NOTE));
    assert_eq!(shaped["messages"][1], empty);
}

/// Only the first `system` message is changed, wherever it is.
#[test]
fn only_the_first_of_two_system_messages_is_changed() {
    let messages = vec![
        said("user", "hello"),
        said("system", "First."),
        said("user", "fix the bug"),
        said("system", "Second."),
    ];
    let mut shaped = with_tools(messages.clone());
    assert!(note_batching(&mut shaped, &unknown()));

    let mut expected = messages;
    expected[1] = said("system", &format!("First.\n\n{AGENTIC_NOTE}"));
    assert_eq!(shaped.to_string(), with_tools(expected).to_string());
}

/// The front of the prompt is the same bytes on a later turn of the same
/// conversation: nothing of the request is in the note.
#[test]
fn the_system_prompt_is_the_same_bytes_on_a_later_turn() {
    let mut first = with_tools(vec![said("system", "Be brief."), said("user", "fix it")]);
    let mut later = with_tools(vec![
        said("system", "Be brief."),
        said("user", "fix it"),
        said("assistant", "reading"),
        said("user", "and the test?"),
    ]);
    later["tools"] = json!([{ "function": { "name": "grep" } }, { "function": { "name": "ls" } }]);
    assert!(note_batching(&mut first, &unknown()));
    assert!(note_batching(&mut later, &unknown()));

    assert_eq!(first["messages"][0], later["messages"][0]);
}

// ── When ──────────────────────────────────────────────────────────────────

/// A body whose first `system` message holds the note, wherever that
/// message is, does not change when the stage runs on it again.
#[test]
fn a_body_whose_system_message_holds_the_note_is_left_alone() {
    for messages in [
        vec![said("system", "Be brief."), said("user", "go")],
        vec![said("user", "go")],
        vec![
            said("user", "hello"),
            said("system", "First."),
            said("user", "go"),
        ],
        vec![
            json!({ "role": "system", "content": [{ "type": "text", "text": "Be brief." }] }),
            said("user", "go"),
        ],
    ] {
        let mut shaped = with_tools(messages);
        assert!(note_batching(&mut shaped, &unknown()));
        let once = shaped.to_string();

        assert!(!note_batching(&mut shaped, &unknown()), "{once}");
        assert_eq!(shaped.to_string(), once);
        assert_eq!(notes(&shaped), 1);
    }
}

/// A copy of the note quoted in a later `tool` result is not the note: the
/// system message still gains it, so the front of the prompt does not change
/// in the middle of a conversation.
#[test]
fn a_copy_of_the_note_in_a_tool_result_does_not_stand_in_for_it() {
    let quoted = vec![
        said("system", "Be brief."),
        said("user", "what does the proxy add?"),
        json!({ "role": "assistant", "tool_calls": [{
            "id": "c1",
            "type": "function",
            "function": { "name": "read_file", "arguments": "{}" },
        }]}),
        json!({ "role": "tool", "tool_call_id": "c1", "content": AGENTIC_NOTE }),
    ];
    let mut shaped = with_tools(quoted.clone());
    assert!(note_batching(&mut shaped, &unknown()));

    let mut expected = quoted;
    expected[0] = said("system", &format!("Be brief.\n\n{AGENTIC_NOTE}"));
    assert_eq!(shaped.to_string(), with_tools(expected).to_string());
    assert_eq!(notes(&shaped), 2);
}

/// A request that draws both notes has stage 1b's first: the client's system
/// prompt, then the drawn-picture note, then this one.
#[test]
fn the_batching_note_follows_the_drawn_picture_note() {
    let drew = "Drew 1 image, 1024x1024 PNG, with flux in 76 s; the user can see it, you cannot.";
    let mut shaped = with_tools(vec![
        said("system", "Be brief."),
        said("user", "draw a fox"),
        json!({ "role": "assistant", "tool_calls": [{
            "id": "c1",
            "type": "function",
            "function": { "name": DRAW_TOOL_NAME, "arguments": "{}" },
        }]}),
        json!({ "role": "tool", "tool_call_id": "c1", "content": drew }),
        said("assistant", "I asked for a red fox in the snow."),
        said("user", "now fix the bug"),
    ]);
    assert!(shape_messages_noting(&mut shaped, &unknown(), true));

    assert_eq!(
        shaped["messages"][0]["content"].as_str().unwrap(),
        format!("Be brief.\n\n{DRAWN_NOTE}\n\n{AGENTIC_NOTE}")
    );
    assert_eq!(shaped["messages"].as_array().unwrap().len(), 6);
}

/// A request without tools is not an agentic turn and is left as it was.
#[test]
fn a_request_without_tools_gets_no_note() {
    let messages = json!([said("system", "Be brief."), said("user", "hello")]);
    for body in [
        json!({ "model": "m", "messages": messages }),
        json!({ "model": "m", "messages": messages, "tools": [] }),
        json!({ "model": "m", "messages": messages, "tools": null }),
        json!({ "model": "m", "messages": messages, "tool_choice": "auto" }),
    ] {
        assert_untouched(&body, &unknown());
    }
}

/// A request with tools and no `messages` array has nowhere to put it.
#[test]
fn a_request_with_no_messages_array_is_left_alone() {
    let mut body = with_tools(vec![]);
    body["messages"] = json!("not an array");
    assert_untouched(&body, &unknown());
}

/// Stage 2b takes the tools from a catalogued model that cannot call them,
/// so a note about tool calls would be about tools the model never sees.
#[test]
fn a_model_whose_tools_will_be_stripped_gets_no_note() {
    let cannot_call = ModelContext {
        capabilities: ModelCapabilities::SUPPORTS_SYSTEM_ROLE,
        catalog_resolved: true,
        ..ModelContext::passthrough()
    };
    let body = with_tools(vec![said("system", "Be brief."), said("user", "go")]);
    assert_untouched(&body, &cannot_call);

    let mut shaped = body.clone();
    apply(&mut shaped, &cannot_call, &on(), None).unwrap();
    assert!(shaped.get("tools").is_none());
    assert_eq!(notes(&shaped), 0);

    let can_call = ModelContext {
        capabilities: ModelCapabilities::SUPPORTS_SYSTEM_ROLE
            | ModelCapabilities::SUPPORTS_TOOL_CALLS,
        ..cannot_call
    };
    let mut shaped = body;
    assert!(note_batching(&mut shaped, &can_call));
}

/// The note is off unless the caller turns it on: the public message
/// shaping never adds it, and the pipeline adds it only for layers that say
/// so.
#[test]
fn the_note_is_added_only_for_a_caller_that_turns_it_on() {
    let body = with_tools(vec![said("system", "Be brief."), said("user", "go")]);

    let mut shaped = body.clone();
    assert!(!shape_messages(&mut shaped, &unknown()));
    assert_eq!(shaped.to_string(), body.to_string());

    let mut off = body.clone();
    apply(&mut off, &unknown(), &SamplingLayers::default(), None).unwrap();
    assert_eq!(off["messages"], body["messages"]);
    assert_eq!(notes(&off), 0);

    let mut told = body;
    apply(&mut told, &unknown(), &on(), None).unwrap();
    assert_eq!(notes(&told), 1);
}

/// The agentic-sampling switch governs sampling changes. The note is not
/// one, and is added with that switch off.
#[test]
fn the_agentic_sampling_switch_does_not_gate_the_note() {
    let layers = on();
    assert!(!layers.agentic_adjustments);
    let mut told = with_tools(vec![said("user", "go")]);
    apply(&mut told, &unknown(), &layers, None).unwrap();

    assert_eq!(told["messages"][0], said("system", AGENTIC_NOTE));
}

// ── The environment switch ────────────────────────────────────────────────

/// Set in the child [`the_environment_switch_does_not_gate_the_note`] runs.
/// Absent in an ordinary run.
const PROBE_ENV: &str = "GGLIB_TEST_AGENTIC_NOTE_PROBE";

/// Run in a child process by the test below, with the kill switch set or
/// removed in that process's environment; a pass-through in an ordinary run.
#[test]
fn the_probe_a_child_process_runs() {
    if std::env::var(PROBE_ENV).is_err() {
        return;
    }
    let mut told = with_tools(vec![said("user", "go")]);
    apply(&mut told, &unknown(), &on(), None).unwrap();

    assert_eq!(told["messages"][0], said("system", AGENTIC_NOTE));
}

/// `GGLIB_DISABLE_AGENTIC_SAMPLING` does not gate the note either.
///
/// The variable is the process's, so the probe runs in a child: this test
/// binary again, filtered to the probe, with the variable truthy and absent.
#[test]
fn the_environment_switch_does_not_gate_the_note() {
    let probe =
        "request_pipeline::agentic_note::agentic_note_tests::the_probe_a_child_process_runs";
    for switch in [Some("1"), None] {
        let mut child = std::process::Command::new(std::env::current_exe().expect("this binary"));
        child
            .args([probe, "--exact", "--test-threads=1"])
            .env(PROBE_ENV, "1")
            .env_remove(DISABLE_AGENTIC_SAMPLING_ENV);
        if let Some(value) = switch {
            child.env(DISABLE_AGENTIC_SAMPLING_ENV, value);
        }
        let out = child.output().expect("running the probe");
        let stdout = String::from_utf8_lossy(&out.stdout);

        assert!(
            out.status.success() && stdout.contains("1 passed"),
            "switch {switch:?} must leave the note on\n{stdout}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
