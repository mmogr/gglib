//! The wire shape of [`AgentEvent`] and the channel sizing.

use super::*;

#[test]
fn agent_event_serde_tag_matches_wire_format() {
    let evt = AgentEvent::FinalAnswer {
        content: "done".into(),
    };
    let json = serde_json::to_value(&evt).unwrap();
    assert_eq!(json["type"], "final_answer");
    assert_eq!(json["content"], "done");
}

#[test]
fn tool_call_start_serialises_correctly() {
    let evt = AgentEvent::ToolCallStart {
        tool_call: ToolCall {
            id: "c1".into(),
            name: "search".into(),
            arguments: serde_json::json!({"q": "rust"}),
        },
        display_name: "Search".into(),
        args_summary: None,
    };
    let json = serde_json::to_value(&evt).unwrap();
    assert_eq!(json["type"], "tool_call_start");
    assert_eq!(json["tool_call"]["name"], "search");
}

/// [`AGENT_EVENT_CHANNEL_CAPACITY`] must be positive and must be at least
/// large enough for a full run at the maximum ceiling configuration
/// (`MAX_ITERATIONS_CEILING` × (`MAX_PARALLEL_TOOLS_CEILING` × 2 + 1) + 1
/// structural events), so that back-pressure never occurs on the hot
/// streaming path for any valid configuration.
#[test]
fn agent_event_channel_capacity_is_sufficient_for_max_config() {
    use super::super::config::{MAX_ITERATIONS_CEILING, MAX_PARALLEL_TOOLS_CEILING};

    // Minimum structural events for a run at ceiling config
    // (no TextDelta headroom included — this is the hard lower bound).
    let structural_per_iter = MAX_PARALLEL_TOOLS_CEILING * 2 + 1;
    let minimum_structural = MAX_ITERATIONS_CEILING * structural_per_iter + 1;
    assert!(
        AGENT_EVENT_CHANNEL_CAPACITY >= minimum_structural,
        "AGENT_EVENT_CHANNEL_CAPACITY ({AGENT_EVENT_CHANNEL_CAPACITY}) is smaller than \
         the minimum required for ceiling config ({minimum_structural}); \
         increase the constant"
    );
}

/// A turn's usage is flat beside its tag, and a count the upstream did not
/// report is absent from the frame, never `0` or `null`.
#[test]
fn turn_usage_serialises_flat_and_leaves_out_what_is_unknown() {
    let evt = AgentEvent::TurnUsage(TurnUsage {
        completion_tokens: Some(12),
        duration_ms: 900,
        ..TurnUsage::default()
    });
    let json = serde_json::to_value(&evt).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"type": "turn_usage", "completion_tokens": 12, "duration_ms": 900})
    );
}

/// A turn's reading and its finish reason sit flat beside the counts, under
/// the names the proxy's usage frame uses, and read back to the same turn.
#[test]
fn turn_usage_carries_its_reading_flat() {
    let usage = TurnUsage {
        prompt_tokens: Some(30),
        duration_ms: 900,
        finish_reason: Some("length".to_owned()),
        reading: crate::domain::agent::ContextReading::new(Some(8192), 2),
        ..TurnUsage::default()
    };
    let json = serde_json::to_value(AgentEvent::TurnUsage(usage.clone())).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "type": "turn_usage",
            "prompt_tokens": 30,
            "duration_ms": 900,
            "finish_reason": "length",
            "context_size": 8192,
            "trimmed_messages": 2,
        })
    );
    let back: TurnUsage = serde_json::from_value(json).unwrap();
    assert_eq!(back, usage);
}

/// A tool's progress on the wire: its counts flat beside the tag, and one it
/// did not report absent.
#[test]
fn tool_progress_serialises_flat_and_leaves_out_what_is_unknown() {
    let evt = AgentEvent::ToolProgress {
        tool_call_id: "c1".into(),
        stage: ToolStage::Sampling,
        pass: Some(1),
        done: Some(2),
        total: Some(4),
        position: None,
    };
    assert_eq!(
        serde_json::to_string(&evt).unwrap(),
        r#"{"type":"tool_progress","tool_call_id":"c1","stage":"sampling","pass":1,"done":2,"total":4}"#
    );
    let queued = AgentEvent::ToolProgress {
        tool_call_id: "c1".into(),
        stage: ToolStage::Queued,
        pass: None,
        done: None,
        total: None,
        position: Some(2),
    };
    assert_eq!(
        serde_json::to_string(&queued).unwrap(),
        r#"{"type":"tool_progress","tool_call_id":"c1","stage":"queued","position":2}"#
    );
}

/// Every stage's wire name.
#[test]
fn tool_stages_are_snake_case() {
    let names: Vec<String> = [
        ToolStage::Queued,
        ToolStage::Loading,
        ToolStage::Sampling,
        ToolStage::Decoding,
        ToolStage::Finishing,
    ]
    .iter()
    .map(|s| serde_json::to_string(s).unwrap())
    .collect();
    assert_eq!(
        names,
        [
            r#""queued""#,
            r#""loading""#,
            r#""sampling""#,
            r#""decoding""#,
            r#""finishing""#
        ]
    );
}

/// A preview frame on the wire.
#[test]
fn tool_preview_serialises_its_frame() {
    let evt = AgentEvent::ToolPreview {
        tool_call_id: "c1".into(),
        frame: PreviewFrame::png(3, 20, "iVBO"),
    };
    assert_eq!(
        serde_json::to_string(&evt).unwrap(),
        r#"{"type":"tool_preview","tool_call_id":"c1","frame":{"mime":"image/png","step":3,"total":20,"b64":"iVBO"}}"#
    );
}

/// A wait on the wire, for each thing waited for.
#[test]
fn waiting_serialises_its_reason() {
    let evt = AgentEvent::Waiting {
        reason: WaitingFor::ImageRender,
        step: 3,
        total: 20,
        position: 1,
    };
    assert_eq!(
        serde_json::to_string(&evt).unwrap(),
        r#"{"type":"waiting","reason":"image_render","step":3,"total":20,"position":1}"#
    );
    let load = AgentEvent::Waiting {
        reason: WaitingFor::ModelLoad,
        step: 0,
        total: 0,
        position: 0,
    };
    assert_eq!(serde_json::to_value(&load).unwrap()["reason"], "model_load");
}

/// An agent run's frames while it draws: a wait, the call, its progress
/// through every stage, and its completion with the image it made. No
/// `tool_preview`: a preview is never a logged frame.
fn drawing_frames() -> Vec<AgentEvent> {
    let progress = |stage, pass, done, position| AgentEvent::ToolProgress {
        tool_call_id: "call_1".into(),
        stage,
        pass,
        done,
        total: done.map(|_| 20),
        position,
    };
    let mut result = ToolResult::text(
        "call_1",
        "Drew 1 image, 1024x1024 PNG, with flux-dev in 76 s; the user can see it, you cannot.",
        true,
    );
    result.images = vec![crate::domain::AttachmentInfo {
        id: crate::domain::AttachmentId::of(b"drawn"),
        mime: "image/png".into(),
        width: 1024,
        height: 1024,
    }];
    vec![
        AgentEvent::Waiting {
            reason: WaitingFor::ImageRender,
            step: 12,
            total: 20,
            position: 1,
        },
        AgentEvent::ToolCallStart {
            tool_call: ToolCall {
                id: "call_1".into(),
                name: "builtin:generate_image".into(),
                arguments: serde_json::json!({ "prompt": "a red fox in fresh snow, morning light" }),
            },
            display_name: "Generate Image".into(),
            args_summary: None,
        },
        progress(ToolStage::Queued, None, None, Some(1)),
        progress(ToolStage::Loading, None, None, None),
        progress(ToolStage::Sampling, Some(1), Some(1), None),
        progress(ToolStage::Sampling, Some(1), Some(20), None),
        progress(ToolStage::Decoding, None, None, None),
        progress(ToolStage::Finishing, None, None, None),
        AgentEvent::ToolCallComplete {
            tool_name: "builtin:generate_image".into(),
            result,
            wait_ms: 0,
            execute_duration_ms: 76_000,
            display_name: "Generate Image".into(),
            duration_display: "76.0s".into(),
        },
    ]
}

/// The checked-in `contracts/runs/tool_progress.json` is exactly these
/// frames, for every client that reads an agent run. Run with
/// `GGLIB_RECORD_CONTRACTS=1` to rewrite it after a deliberate change.
#[test]
fn drawing_frames_match_the_checked_in_contract() {
    let frames: Vec<serde_json::Value> = drawing_frames()
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect();
    let mut want = serde_json::to_string_pretty(&frames).expect("serialise");
    want.push('\n');
    assert!(!want.contains("tool_preview"));
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/runs/tool_progress.json");
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::write(&path, &want).expect("write tool_progress.json");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/runs/tool_progress.json");
    assert!(
        have == want,
        "contracts/runs/tool_progress.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
    );
}
