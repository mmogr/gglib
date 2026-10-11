//! A tool's progress and preview in an agent run: progress is logged like
//! any event; a preview frame is kept beside the log, reaches a reader
//! between its call's start and completion, and is never logged. The chat
//! route carries no preview.

use std::time::Duration;

use futures_util::StreamExt as _;
use gglib_core::domain::agent::{AgentEvent, PreviewFrame, ToolCall, ToolResult, ToolStage};
use gglib_core::ports::{RunEvent, RunEvents, RunsPort as _};
use serde_json::json;

use super::run_fixture::{End, LOCAL, logged, paced, settled, start, state};
use super::sse_event;

fn tool_start() -> AgentEvent {
    AgentEvent::ToolCallStart {
        tool_call: ToolCall {
            id: "c1".to_owned(),
            name: "builtin:generate_image".to_owned(),
            arguments: json!({ "prompt": "a fox" }),
        },
        display_name: "Generate Image".to_owned(),
        args_summary: None,
    }
}

fn tool_progress(done: u32) -> AgentEvent {
    AgentEvent::ToolProgress {
        tool_call_id: "c1".to_owned(),
        stage: ToolStage::Sampling,
        pass: Some(1),
        done: Some(done),
        total: Some(4),
        position: None,
    }
}

fn tool_preview(step: u32) -> AgentEvent {
    AgentEvent::ToolPreview {
        tool_call_id: "c1".to_owned(),
        frame: PreviewFrame::png(step, 4, "PREVIEW-BYTES"),
    }
}

fn tool_complete() -> AgentEvent {
    AgentEvent::ToolCallComplete {
        tool_name: "builtin:generate_image".to_owned(),
        result: ToolResult::text("c1", "Drew 1 image.", true),
        wait_ms: 0,
        execute_duration_ms: 1,
        display_name: "Generate Image".to_owned(),
        duration_display: "1ms".to_owned(),
    }
}

fn drawing() -> Vec<AgentEvent> {
    vec![
        tool_start(),
        tool_progress(1),
        tool_preview(1),
        tool_progress(2),
        tool_preview(2),
        tool_complete(),
        AgentEvent::FinalAnswer {
            content: "I drew a fox.".to_owned(),
        },
    ]
}

/// Everything a reader is given, to the run's end.
async fn read_all(mut events: RunEvents) -> Vec<RunEvent> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut items = Vec::new();
        while let Some(item) = events.next().await {
            items.push(item);
        }
        items
    })
    .await
    .expect("the run ends within five seconds")
}

fn frame_text(item: &RunEvent) -> Option<&str> {
    match item {
        RunEvent::Frame { data, .. } => Some(data),
        _ => None,
    }
}

#[tokio::test]
async fn a_runs_log_holds_the_progress_and_no_preview() {
    let (_dir, state) = state().await;
    let (p, _) = paced(drawing(), End::Finish, Duration::from_millis(20));
    start(&state, "a1", None, p).await;
    let live = state.runs.events(&LOCAL, "a1", 0).unwrap();

    let items = read_all(live).await;
    settled(&state).await;

    let frames: Vec<&str> = items.iter().filter_map(frame_text).collect();
    assert_eq!(frames.len(), 5, "start, two progress, complete, answer");
    assert_eq!(
        frames
            .iter()
            .filter(|f| f.contains(r#""type":"tool_progress""#))
            .count(),
        2
    );
    let log = read_all(state.runs.events(&LOCAL, "a1", 0).unwrap()).await;
    let bytes: String = log.iter().filter_map(frame_text).collect();
    assert!(
        !bytes.contains("tool_preview") && !bytes.contains("PREVIEW-BYTES"),
        "{bytes}"
    );
    assert!(
        log.iter().all(|i| !matches!(i, RunEvent::Preview { .. })),
        "an ended run hands out no preview"
    );
}

#[tokio::test]
async fn a_live_reader_gets_previews_only_between_the_calls_start_and_completion() {
    let (_dir, state) = state().await;
    let (p, _) = paced(drawing(), End::Finish, Duration::from_millis(20));
    start(&state, "a1", None, p).await;
    let live = state.runs.events(&LOCAL, "a1", 0).unwrap();

    let items = read_all(live).await;
    settled(&state).await;

    let at = |kind: &str| {
        items
            .iter()
            .position(|i| frame_text(i).is_some_and(|f| f.contains(kind)))
            .expect(kind)
    };
    let (started, completed) = (at("tool_call_start"), at("tool_call_complete"));
    let previews: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i, RunEvent::Preview { .. }))
        .map(|(n, _)| n)
        .collect();
    assert!(!previews.is_empty(), "a live reader sees the preview");
    assert!(
        previews.iter().all(|&n| started < n && n < completed),
        "{previews:?} not inside {started}..{completed}"
    );
    let RunEvent::Preview { tool_call_id, data } = &items[previews[0]] else {
        unreachable!()
    };
    assert_eq!(&**tool_call_id, "c1");
    assert!(data.starts_with(r#"{"tool_call_id":"c1","frame":{"mime":"image/png""#));
}

#[tokio::test]
async fn a_reader_that_joins_after_the_calls_completion_gets_no_preview() {
    let (_dir, state) = state().await;
    let events = vec![tool_start(), tool_preview(1), tool_complete()];
    let (p, _) = paced(events, End::Hang, Duration::ZERO);
    start(&state, "a1", None, p).await;
    logged(&state, "a1", 2).await;
    let mut joining = state.runs.events(&LOCAL, "a1", 2).unwrap();

    let got = tokio::time::timeout(Duration::from_millis(200), joining.next()).await;

    assert!(got.is_err(), "expected nothing, got {got:?}");
    state.runs.cancel(&LOCAL, "a1").unwrap();
    settled(&state).await;
}

#[tokio::test]
async fn another_calls_completion_leaves_the_render_its_preview() {
    let (_dir, state) = state().await;
    let clock = AgentEvent::ToolCallComplete {
        tool_name: "builtin:get_current_time".to_owned(),
        result: ToolResult::text("c2", "12:00", true),
        wait_ms: 0,
        execute_duration_ms: 1,
        display_name: "Get Current Time".to_owned(),
        duration_display: "1ms".to_owned(),
    };
    let events = vec![tool_start(), tool_preview(1), clock];
    let (p, _) = paced(events, End::Hang, Duration::ZERO);
    start(&state, "a1", None, p).await;
    logged(&state, "a1", 2).await;
    let mut joining = state.runs.events(&LOCAL, "a1", 2).unwrap();

    let got = tokio::time::timeout(Duration::from_secs(2), joining.next()).await;

    let Ok(Some(RunEvent::Preview { tool_call_id, .. })) = got else {
        panic!("expected c1's preview, got {got:?}");
    };
    assert_eq!(&*tool_call_id, "c1");
    state.runs.cancel(&LOCAL, "a1").unwrap();
    settled(&state).await;
}

#[test]
fn the_chat_route_carries_progress_and_drops_the_preview() {
    assert!(sse_event(&tool_preview(1)).is_none());
    assert!(sse_event(&tool_progress(1)).is_some());
}
