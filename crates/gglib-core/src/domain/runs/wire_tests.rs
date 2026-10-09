//! The wire shapes of a run, pinned against the recorded bodies both clients
//! replay, and the frames of a reply whose tool made an image.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{RunError, RunInfo, RunKind, RunList, RunStatus};
use crate::domain::agent::{AgentEvent, ToolCall, ToolResult, rows_from_frames};
use crate::domain::attachment::{AttachmentId, AttachmentInfo};
use crate::domain::chat::MessageRole;

/// The recorded bodies, by name. The field order is the file's order.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recorded {
    queued: RunInfo,
    in_progress: RunInfo,
    completed: RunInfo,
    failed: RunInfo,
    cancelled: RunInfo,
    /// The frames of a reply whose tool made an image, as a run's events
    /// carry them. `AgentEvent` is written, never read, so each is kept as
    /// its JSON.
    tool_reply: Vec<Value>,
    list: RunList,
}

const CREATED: u64 = 1_790_000_000_000;

fn run(id: &str, kind: RunKind, status: RunStatus) -> RunInfo {
    RunInfo {
        id: id.to_owned(),
        kind,
        status,
        model: None,
        device: None,
        created_at_ms: CREATED,
        finished_at_ms: None,
        conversation_id: None,
        last_seq: 0,
        error: None,
    }
}

/// The image the recorded tool made: a 1024x1024 PNG, the same image
/// `contracts/chats/recorded.json`'s `tool_reply` rows carry. The id is the
/// hash of this text, standing in for the image's bytes.
fn drawing() -> AttachmentInfo {
    AttachmentInfo {
        id: AttachmentId::of(b"contracts: a red dot a tool drew"),
        mime: "image/png".to_owned(),
        width: 1024,
        height: 1024,
    }
}

/// A reply that calls a tool which makes one image, then answers.
fn tool_reply() -> Vec<AgentEvent> {
    let call = ToolCall {
        id: "call-draw-1".to_owned(),
        name: "draw".to_owned(),
        arguments: json!({ "prompt": "a red dot" }),
    };
    let mut result = ToolResult::text("call-draw-1", "[image 1024x1024 PNG stored]", true);
    result.images = vec![drawing()];
    vec![
        AgentEvent::ToolCallStart {
            tool_call: call,
            display_name: "Draw".to_owned(),
            args_summary: None,
        },
        AgentEvent::ToolCallComplete {
            tool_name: "draw".to_owned(),
            result,
            wait_ms: 0,
            execute_duration_ms: 900,
            display_name: "Draw".to_owned(),
            duration_display: "900ms".to_owned(),
        },
        AgentEvent::IterationComplete {
            iteration: 1,
            tool_calls: 1,
        },
        AgentEvent::TextDelta {
            content: "Here is a red dot.".to_owned(),
        },
        AgentEvent::FinalAnswer {
            content: "Here is a red dot.".to_owned(),
        },
    ]
}

fn recorded() -> Recorded {
    let queued = run("run-queued-01", RunKind::Chat, RunStatus::Queued);
    let in_progress = RunInfo {
        model: Some("qwen3-8b".to_owned()),
        device: Some("phone-7c2e".to_owned()),
        last_seq: 42,
        ..run("run-busy-02", RunKind::Agent, RunStatus::InProgress)
    };
    let completed = RunInfo {
        model: Some("qwen3-8b".to_owned()),
        finished_at_ms: Some(CREATED + 8_250),
        last_seq: 311,
        ..run("run-done-03", RunKind::Chat, RunStatus::Completed)
    };
    let failed = RunInfo {
        model: Some("gemma-3-12b".to_owned()),
        device: Some("phone-7c2e".to_owned()),
        finished_at_ms: Some(CREATED + 1_500),
        last_seq: 3,
        error: Some(RunError {
            code: "model_unavailable".to_owned(),
            message: "The model could not be loaded.".to_owned(),
        }),
        ..run("run-failed-04", RunKind::Agent, RunStatus::Failed)
    };
    let cancelled = RunInfo {
        finished_at_ms: Some(CREATED + 600),
        last_seq: 7,
        ..run("run-cancelled-05", RunKind::Chat, RunStatus::Cancelled)
    };
    let list = RunList {
        runs: vec![in_progress.clone(), failed.clone()],
    };
    Recorded {
        queued,
        in_progress,
        completed,
        failed,
        cancelled,
        tool_reply: tool_reply()
            .iter()
            .map(|event| serde_json::to_value(event).unwrap())
            .collect(),
        list,
    }
}

fn recorded_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/runs/recorded.json")
}

/// The checked-in file is exactly what these types emit. Run with
/// `GGLIB_RECORD_CONTRACTS=1` to rewrite it after a deliberate change.
#[test]
fn recorded_bodies_match_the_checked_in_file() {
    let mut want = serde_json::to_string_pretty(&recorded()).expect("serialise");
    want.push('\n');
    let path = recorded_path();
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::write(&path, &want).expect("write recorded.json");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/runs/recorded.json");
    assert!(
        have == want,
        "contracts/runs/recorded.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
    );
}

#[test]
fn every_recorded_body_decodes_back_to_its_value() {
    let file = std::fs::read_to_string(recorded_path()).expect("read recorded.json");
    let decoded: Recorded = serde_json::from_str(&file).expect("decode recorded.json");
    assert_eq!(decoded, recorded());
}

/// Rust leaves a `None` out of the body rather than writing `null`.
#[test]
fn a_none_is_emitted_as_an_absent_key() {
    let body = serde_json::to_value(run("r", RunKind::Chat, RunStatus::Queued)).unwrap();
    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["created_at_ms", "id", "kind", "last_seq", "status"],
        "{body}"
    );
}

#[test]
fn an_absent_key_and_a_null_both_decode_to_none() {
    let base = json!({
        "id": "r", "kind": "chat", "status": "queued",
        "created_at_ms": CREATED, "last_seq": 0,
    });
    let mut nulls = base.clone();
    for key in ["model", "device", "finished_at_ms", "error"] {
        nulls[key] = Value::Null;
    }
    let want = run("r", RunKind::Chat, RunStatus::Queued);
    assert_eq!(serde_json::from_value::<RunInfo>(base).unwrap(), want);
    assert_eq!(serde_json::from_value::<RunInfo>(nulls).unwrap(), want);
}

#[test]
fn statuses_and_kinds_are_snake_case_on_the_wire() {
    let names = [
        (RunStatus::Queued, "queued"),
        (RunStatus::InProgress, "in_progress"),
        (RunStatus::Completed, "completed"),
        (RunStatus::Failed, "failed"),
        (RunStatus::Cancelled, "cancelled"),
    ];
    for (status, name) in names {
        assert_eq!(serde_json::to_value(status).unwrap(), json!(name));
    }
    assert_eq!(serde_json::to_value(RunKind::Chat).unwrap(), json!("chat"));
    assert_eq!(
        serde_json::to_value(RunKind::Agent).unwrap(),
        json!("agent")
    );
}

#[test]
fn only_completed_failed_and_cancelled_are_terminal() {
    assert!(!RunStatus::Queued.is_terminal());
    assert!(!RunStatus::InProgress.is_terminal());
    assert!(RunStatus::Completed.is_terminal());
    assert!(RunStatus::Failed.is_terminal());
    assert!(RunStatus::Cancelled.is_terminal());
}

#[test]
fn a_conversation_id_is_a_number_and_absent_when_there_is_none() {
    let with = RunInfo {
        conversation_id: Some(42),
        ..run("run-agent-06", RunKind::Agent, RunStatus::Queued)
    };
    let wire = serde_json::to_value(&with).unwrap();
    assert_eq!(wire["conversation_id"], json!(42));
    assert_eq!(serde_json::from_value::<RunInfo>(wire).unwrap(), with);

    let without = serde_json::to_value(run("r", RunKind::Chat, RunStatus::Queued)).unwrap();
    assert!(without.get("conversation_id").is_none(), "{without}");
}

/// A run holds the conversation it writes for as long as it is reported
/// going, and no other conversation, and none once it is reported ended.
#[test]
fn a_run_holds_its_conversation_until_it_is_reported_ended() {
    for (status, held) in [
        (RunStatus::Queued, true),
        (RunStatus::InProgress, true),
        (RunStatus::Completed, false),
        (RunStatus::Failed, false),
        (RunStatus::Cancelled, false),
    ] {
        let on_seven = RunInfo {
            conversation_id: Some(7),
            ..run("r", RunKind::Agent, status)
        };
        assert_eq!(on_seven.holds(7), held, "{status:?}");
        assert!(!on_seven.holds(8), "{status:?}");
        assert!(!run("r", RunKind::Agent, status).holds(7), "{status:?}");
    }
}

/// The recorded `tool_call_complete` frame lists the image its tool made,
/// by id with its facts, and its text names the image; the reply's rows
/// saved from these frames put the image's id on the tool row alone.
#[test]
fn a_tool_frame_carries_its_image_and_the_saved_tool_row_its_id() {
    let frames = recorded().tool_reply;
    let image = drawing();
    assert_eq!(
        frames[1]["result"],
        json!({
            "tool_call_id": "call-draw-1",
            "content": "[image 1024x1024 PNG stored]",
            "success": true,
            "images": [{
                "id": image.id.as_str(),
                "mime": "image/png",
                "width": 1024,
                "height": 1024,
            }],
        })
    );

    let lines: Vec<String> = frames.iter().map(Value::to_string).collect();
    let rows = rows_from_frames(lines.iter().map(String::as_str), true, 12);
    let roles: Vec<MessageRole> = rows.iter().map(|row| row.role).collect();
    assert_eq!(
        roles,
        [
            MessageRole::Assistant,
            MessageRole::Tool,
            MessageRole::Assistant
        ]
    );
    assert_eq!(rows[1].content, "[image 1024x1024 PNG stored]");
    assert_eq!(rows[1].images, [image.id]);
    assert!(rows[0].images.is_empty() && rows[2].images.is_empty());
}
