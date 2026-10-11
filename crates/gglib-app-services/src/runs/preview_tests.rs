//! A run's preview frame: beside the log, never in it; sent only to a
//! reader that has caught up; the current one once to a reader that joins;
//! gone once its call completes or the run ends.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt as _;
use gglib_core::domain::agent::PreviewFrame;
use gglib_core::domain::runs::{RunKind, RunStatus};
use gglib_core::ports::{RunEvent, RunEvents, RunScope, RunsPort};

use super::cell::{RunCell, RunSpec, Step};
use super::test_executor::{Cmd, body, next, registry, until};

const LOCAL: RunScope = RunScope::Local;

/// What `RunLog::preview` keeps for call `c1` at `step` of 20.
fn preview_data(step: u32) -> String {
    format!(
        r#"{{"tool_call_id":"c1","frame":{{"mime":"image/png","step":{step},"total":20,"b64":"iVBO"}}}}"#
    )
}

fn frame(step: u32) -> PreviewFrame {
    PreviewFrame::png(step, 20, "iVBO")
}

fn logged(seq: u32, data: &str) -> RunEvent {
    RunEvent::Frame {
        seq,
        data: data.into(),
    }
}

fn preview(step: u32) -> RunEvent {
    RunEvent::Preview {
        tool_call_id: "c1".into(),
        data: preview_data(step).into(),
    }
}

/// Nothing more arrives: with the clock paused, a second's timeout fires as
/// soon as every task is idle.
async fn nothing_more(events: &mut RunEvents) {
    let got = tokio::time::timeout(Duration::from_secs(1), events.next()).await;
    assert!(got.is_err(), "expected nothing, got {got:?}");
}

#[tokio::test(start_paused = true)]
async fn a_reader_that_is_behind_gets_every_frame_before_the_preview() {
    let (runs, executor, _) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    for data in ["start", "f2", "f3"] {
        script.send(Cmd::Frame(data.into())).unwrap();
    }
    script.send(Cmd::Preview("c1", frame(3))).unwrap();
    let cell = Arc::clone(&runs.lock().runs["r1"]);
    until(|| waits_with_preview_at(&cell, 3)).await;

    let mut events = runs.events(&LOCAL, "r1", 0).unwrap();

    assert_eq!(next(&mut events).await, Some(logged(1, "start")));
    assert_eq!(next(&mut events).await, Some(logged(2, "f2")));
    assert_eq!(next(&mut events).await, Some(logged(3, "f3")));
    assert_eq!(next(&mut events).await, Some(preview(3)));
    nothing_more(&mut events).await;
}

#[tokio::test(start_paused = true)]
async fn a_live_reader_gets_a_preview_after_the_frame_logged_before_it_and_each_new_one() {
    let (runs, executor, _) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    let mut events = runs.events(&LOCAL, "r1", 0).unwrap();

    script.send(Cmd::Frame("start".into())).unwrap();
    script.send(Cmd::Preview("c1", frame(1))).unwrap();
    assert_eq!(next(&mut events).await, Some(logged(1, "start")));
    assert_eq!(next(&mut events).await, Some(preview(1)));

    script.send(Cmd::Preview("c1", frame(2))).unwrap();
    assert_eq!(next(&mut events).await, Some(preview(2)));
    nothing_more(&mut events).await;
}

#[tokio::test(start_paused = true)]
async fn a_reader_that_joins_at_any_cursor_gets_the_current_frame_once() {
    let (runs, executor, _) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    script.send(Cmd::Frame("start".into())).unwrap();
    script.send(Cmd::Preview("c1", frame(5))).unwrap();
    let cell = Arc::clone(&runs.lock().runs["r1"]);
    until(|| waits_with_preview_at(&cell, 1)).await;

    let mut from_start = runs.events(&LOCAL, "r1", 0).unwrap();
    let mut caught_up = runs.events(&LOCAL, "r1", 1).unwrap();

    assert_eq!(next(&mut from_start).await, Some(logged(1, "start")));
    assert_eq!(next(&mut from_start).await, Some(preview(5)));
    nothing_more(&mut from_start).await;
    assert_eq!(next(&mut caught_up).await, Some(preview(5)));
    nothing_more(&mut caught_up).await;
}

#[tokio::test(start_paused = true)]
async fn no_preview_goes_out_once_it_is_cleared() {
    let (runs, executor, _) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    let mut live = runs.events(&LOCAL, "r1", 0).unwrap();
    script.send(Cmd::Frame("start".into())).unwrap();
    script.send(Cmd::Preview("c1", frame(1))).unwrap();
    assert_eq!(next(&mut live).await, Some(logged(1, "start")));
    assert_eq!(next(&mut live).await, Some(preview(1)));

    script
        .send(Cmd::Completes("complete".into(), "c1"))
        .unwrap();
    script.send(Cmd::Frame("answer".into())).unwrap();

    assert_eq!(next(&mut live).await, Some(logged(2, "complete")));
    assert_eq!(next(&mut live).await, Some(logged(3, "answer")));
    nothing_more(&mut live).await;
    let mut joining = runs.events(&LOCAL, "r1", 3).unwrap();
    nothing_more(&mut joining).await;
}

#[tokio::test(start_paused = true)]
async fn a_preview_changes_neither_the_logs_bytes_nor_its_last_seq() {
    let (runs, executor, _) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    script.send(Cmd::Frame("start".into())).unwrap();
    until(|| runs.get(&LOCAL, "r1").unwrap().last_seq == 1).await;
    let cell = Arc::clone(&runs.lock().runs["r1"]);
    let (bytes, frames) = (cell.bytes(), cell.frames());

    script.send(Cmd::Preview("c1", frame(1))).unwrap();
    script.send(Cmd::Preview("c1", frame(2))).unwrap();
    let mut events = runs.events(&LOCAL, "r1", 1).unwrap();
    assert_eq!(next(&mut events).await, Some(preview(2)));

    assert_eq!(cell.bytes(), bytes);
    assert_eq!(cell.frames(), frames);
    assert_eq!(runs.get(&LOCAL, "r1").unwrap().last_seq, 1);
    let log: String = cell.frames().iter().map(ToString::to_string).collect();
    assert!(!log.contains("iVBO") && !log.contains("preview"), "{log}");
}

#[tokio::test(start_paused = true)]
async fn another_calls_completion_keeps_the_preview_and_its_own_clears_it() {
    let (runs, executor, _) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    let mut live = runs.events(&LOCAL, "r1", 0).unwrap();
    script.send(Cmd::Frame("start".into())).unwrap();
    script.send(Cmd::Preview("c1", frame(1))).unwrap();
    assert_eq!(next(&mut live).await, Some(logged(1, "start")));
    assert_eq!(next(&mut live).await, Some(preview(1)));

    script
        .send(Cmd::Completes("c2 complete".into(), "c2"))
        .unwrap();
    assert_eq!(next(&mut live).await, Some(logged(2, "c2 complete")));
    let cell = Arc::clone(&runs.lock().runs["r1"]);
    until(|| waits_with_preview_at(&cell, 2)).await;
    let mut joining = runs.events(&LOCAL, "r1", 2).unwrap();
    assert_eq!(next(&mut joining).await, Some(preview(1)));

    script
        .send(Cmd::Completes("c1 complete".into(), "c1"))
        .unwrap();
    assert_eq!(next(&mut live).await, Some(logged(3, "c1 complete")));
    until(|| matches!(cell.step(3), Step::Wait(None))).await;
    let mut late = runs.events(&LOCAL, "r1", 3).unwrap();
    nothing_more(&mut late).await;
}

/// A call's completion forgets that call's preview in the step that logs
/// it, and leaves another call's: whoever has read the completion frame
/// is handed no frame of the finished call.
#[test]
fn a_calls_completion_forgets_its_own_preview_as_it_is_logged() {
    let cell = cell(false);
    cell.preview("c1", preview_data(1));

    cell.append_completing("c2 done".to_owned(), "c2").unwrap();
    assert!(
        waits_with_preview_at(&cell, 1),
        "another call's frame stays"
    );

    cell.append_completing("c1 done".to_owned(), "c1").unwrap();
    assert!(matches!(cell.step(1), Step::Frames(_)));
    assert!(matches!(cell.step(2), Step::Wait(None)));
    assert_eq!(cell.frames().len(), 2);
}

fn cell(awaits_end: bool) -> RunCell {
    let spec = RunSpec {
        kind: RunKind::Agent,
        model: None,
        conversation_id: None,
    };
    RunCell::new("r1", LOCAL, 0, spec, awaits_end, Arc::new(|| 1_000))
}

/// Whether a reader that has read `cursor` frames would be handed a preview.
fn waits_with_preview_at(cell: &RunCell, cursor: usize) -> bool {
    matches!(cell.step(cursor), Step::Wait(Some(_)))
}

/// A run that has ended (by `finish`, which `cancel` and `drop_now` also
/// call) but whose end is still being handled shows readers a wait: the wait carries no preview, and a late one is refused.
#[test]
fn the_end_forgets_the_preview_and_refuses_a_new_one() {
    let cell = cell(true);
    cell.preview("c1", preview_data(1));
    assert!(waits_with_preview_at(&cell, 0));

    assert!(cell.finish(RunStatus::Completed, None));

    assert!(matches!(cell.step(0), Step::Wait(None)));
    cell.preview("c1", preview_data(2));
    assert!(matches!(cell.step(0), Step::Wait(None)));
}
