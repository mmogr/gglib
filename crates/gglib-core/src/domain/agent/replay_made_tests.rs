//! How each turn was made, as its saved row says it: only what the turn's
//! [`TurnUsage`] had, under [`MADE_KEYS`], and never a figure it lacked.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use super::*;
use crate::domain::agent::AgentEvent;
use crate::domain::chat::MessageRole;

fn rows(events: &[AgentEvent], finished: bool) -> Vec<NewMessage> {
    let frames: Vec<String> = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect();
    rows_from_frames(frames.iter().map(String::as_str), finished, 9)
}

fn text(content: &str) -> AgentEvent {
    AgentEvent::TextDelta {
        content: content.to_owned(),
    }
}

fn full_usage() -> TurnUsage {
    TurnUsage {
        model: Some("Qwen3.8-27B".to_owned()),
        quantization: Some("Q8_0".to_owned()),
        prompt_tokens: Some(3180),
        cached_tokens: Some(2100),
        completion_tokens: Some(496),
        duration_ms: 41_000,
        writing_ms: Some(38_200),
        device: Some("phone-7c2e".to_owned()),
    }
}

/// An assistant row's metadata but for what other parts of the replay own
/// (reasoning, its time, the unfinished mark, tool calls): any key saved
/// for how the turn was made shows up here, wanted or not.
fn made(row: &NewMessage) -> Map<String, Value> {
    let owned_elsewhere = [
        THINKING_KEY,
        THINKING_DURATION_KEY,
        INCOMPLETE_KEY,
        "tool_calls",
    ];
    match row.metadata.clone() {
        Some(Value::Object(meta)) => meta
            .into_iter()
            .filter(|(key, _)| !owned_elsewhere.contains(&key.as_str()))
            .collect(),
        _ => Map::new(),
    }
}

fn assistants(rows: &[NewMessage]) -> Vec<&NewMessage> {
    rows.iter()
        .filter(|r| r.role == MessageRole::Assistant)
        .collect()
}

/// The frames of one finished turn with usage, and its saved row's made
/// metadata: the page's contract, in `contracts/runs/turn_made.json`.
fn turn_made() -> Value {
    let events = [
        text("It restarts."),
        AgentEvent::TurnUsage(full_usage()),
        AgentEvent::FinalAnswer {
            content: "It restarts.".to_owned(),
        },
    ];
    let saved = rows(&events, true);
    let frames: Vec<Value> = events
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect();
    json!({ "frames": frames, "metadata": made(assistants(&saved)[0]) })
}

#[test]
fn a_turn_with_usage_saves_each_figure_under_its_key() {
    let saved = turn_made();
    assert_eq!(
        saved["metadata"],
        json!({
            "modelName": "Qwen3.8-27B",
            "modelQuantization": "Q8_0",
            "promptTokens": 3180,
            "cachedTokens": 2100,
            "completionTokens": 496,
            "turnDurationMs": 41_000,
            "writingDurationMs": 38_200,
            "device": "phone-7c2e",
        })
    );
}

/// No rate is saved: the page computes it from what is.
#[test]
fn nothing_derived_is_saved() {
    let saved = turn_made();
    let keys: Vec<&String> = saved["metadata"].as_object().unwrap().keys().collect();
    assert!(
        keys.iter()
            .all(|k| !k.to_lowercase().contains("rate") && !k.contains("PerSecond"))
    );
}

#[test]
fn what_the_upstream_did_not_report_is_left_out_not_zero() {
    let usage = TurnUsage {
        duration_ms: 1200,
        ..TurnUsage::default()
    };
    let saved = rows(&[text("hi"), AgentEvent::TurnUsage(usage), text("")], true);
    let row = assistants(&saved)[0];
    assert_eq!(Value::Object(made(row)), json!({ "turnDurationMs": 1200 }));
}

#[test]
fn a_turn_without_usage_saves_no_figure() {
    let saved = rows(
        &[
            text("hi"),
            AgentEvent::FinalAnswer {
                content: "hi".to_owned(),
            },
        ],
        true,
    );
    assert!(made(assistants(&saved)[0]).is_empty());
}

/// Cancelled mid-turn: the stream never ended, so no usage came, and the
/// unfinished row claims nothing about how it was made.
#[test]
fn a_turn_cancelled_before_its_stream_ended_saves_no_figure() {
    let saved = rows(&[text("half an ans")], false);
    let row = assistants(&saved)[0];
    assert!(made(row).is_empty());
    assert_eq!(row.metadata.as_ref().unwrap()[INCOMPLETE_KEY], json!(true));
}

/// Each turn keeps its own figures: the second turn's never land on the first.
#[test]
fn several_turns_each_save_their_own() {
    let first = TurnUsage {
        prompt_tokens: Some(100),
        duration_ms: 10,
        ..TurnUsage::default()
    };
    let second = TurnUsage {
        prompt_tokens: Some(250),
        duration_ms: 20,
        ..TurnUsage::default()
    };
    let saved = rows(
        &[
            text("looking"),
            AgentEvent::TurnUsage(first),
            AgentEvent::IterationComplete {
                iteration: 1,
                tool_calls: 0,
            },
            AgentEvent::TurnUsage(second),
            AgentEvent::FinalAnswer {
                content: "done".to_owned(),
            },
        ],
        true,
    );
    let made: Vec<Value> = assistants(&saved)
        .iter()
        .map(|r| Value::Object(made(r)))
        .collect();
    assert_eq!(
        made,
        [
            json!({ "promptTokens": 100, "turnDurationMs": 10 }),
            json!({ "promptTokens": 250, "turnDurationMs": 20 }),
        ]
    );
}

fn contract_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/runs/turn_made.json")
}

/// The page reads these frames and this metadata and must say the same of
/// both. `GGLIB_RECORD_CONTRACTS=1` rewrites the file after a deliberate change.
#[test]
fn the_turn_made_contract_is_current() {
    let want = serde_json::to_string_pretty(&turn_made()).unwrap() + "\n";
    let path = contract_path();
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::write(&path, &want).expect("write turn_made.json");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/runs/turn_made.json");
    assert_eq!(
        have, want,
        "contracts/runs/turn_made.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1"
    );
}

/// A second turn that brought no usage (here cancelled mid-stream) says
/// nothing of how it was made: the first turn's figures stay with the first.
#[test]
fn a_turn_without_usage_never_carries_the_previous_turns_figures() {
    let saved = rows(
        &[
            text("looking"),
            AgentEvent::TurnUsage(full_usage()),
            AgentEvent::IterationComplete {
                iteration: 1,
                tool_calls: 0,
            },
            text("half an ans"),
        ],
        false,
    );
    let replies = assistants(&saved);
    assert_eq!(replies.len(), 2);
    assert!(!made(replies[0]).is_empty());
    assert!(
        made(replies[1]).is_empty(),
        "turn 2 got {:?}",
        made(replies[1])
    );
}
