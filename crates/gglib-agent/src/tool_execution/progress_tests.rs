//! The rate limit on a tool's progress, on a clock the test moves.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gglib_core::AgentEvent;
use gglib_core::domain::agent::{PreviewFrame, ToolProgressSink, ToolProgressUpdate, ToolStage};
use tokio::sync::mpsc;

use super::{Clock, RateLimitedSink};

/// A clock at a fixed start plus however many ms the test has moved it.
fn hand_clock() -> (Clock, Arc<AtomicU64>) {
    let start = Instant::now();
    let ms = Arc::new(AtomicU64::new(0));
    let moved = Arc::clone(&ms);
    let clock: Clock =
        Arc::new(move || start + Duration::from_millis(moved.load(Ordering::SeqCst)));
    (clock, ms)
}

fn sampling(pass: u32, done: u32, total: u32) -> ToolProgressUpdate {
    ToolProgressUpdate {
        pass: Some(pass),
        done: Some(done),
        total: Some(total),
        ..ToolProgressUpdate::stage(ToolStage::Sampling)
    }
}

fn drain(rx: &mut mpsc::Receiver<AgentEvent>) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

/// `(stage, pass, done)` of each `ToolProgress` sent.
fn progress(events: &[AgentEvent]) -> Vec<(ToolStage, Option<u32>, Option<u32>)> {
    events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::ToolProgress {
                stage, pass, done, ..
            } => Some((*stage, *pass, *done)),
            _ => None,
        })
        .collect()
}

#[test]
fn ten_steps_in_a_second_send_one_and_every_stage_change_goes_at_once() {
    let (tx, mut rx) = mpsc::channel(64);
    let (clock, ms) = hand_clock();
    let sink = RateLimitedSink::new("c1".into(), tx, clock);

    sink.progress(ToolProgressUpdate::stage(ToolStage::Loading));
    for step in 1..=10 {
        sink.progress(sampling(1, step, 20));
        ms.fetch_add(90, Ordering::SeqCst);
    }
    sink.progress(ToolProgressUpdate::stage(ToolStage::Decoding));

    assert_eq!(
        progress(&drain(&mut rx)),
        [
            (ToolStage::Loading, None, None),
            (ToolStage::Sampling, Some(1), Some(1)),
            (ToolStage::Decoding, None, None),
        ]
    );
}

#[test]
fn a_step_goes_out_once_a_second_has_passed_since_the_last_sent() {
    let (tx, mut rx) = mpsc::channel(64);
    let (clock, ms) = hand_clock();
    let sink = RateLimitedSink::new("c1".into(), tx, clock);

    sink.progress(sampling(1, 1, 20));
    ms.store(999, Ordering::SeqCst);
    sink.progress(sampling(1, 2, 20));
    ms.store(1_000, Ordering::SeqCst);
    sink.progress(sampling(1, 3, 20));
    ms.store(1_500, Ordering::SeqCst);
    sink.progress(sampling(1, 4, 20));

    assert_eq!(
        progress(&drain(&mut rx)),
        [
            (ToolStage::Sampling, Some(1), Some(1)),
            (ToolStage::Sampling, Some(1), Some(3)),
        ]
    );
}

#[test]
fn a_new_pass_and_a_passs_last_step_go_out_at_once() {
    let (tx, mut rx) = mpsc::channel(64);
    let (clock, _ms) = hand_clock();
    let sink = RateLimitedSink::new("c1".into(), tx, clock);

    sink.progress(sampling(1, 1, 4));
    sink.progress(sampling(1, 2, 4));
    sink.progress(sampling(1, 4, 4));
    sink.progress(sampling(1, 4, 4));
    sink.progress(sampling(2, 1, 4));

    assert_eq!(
        progress(&drain(&mut rx)),
        [
            (ToolStage::Sampling, Some(1), Some(1)),
            (ToolStage::Sampling, Some(1), Some(4)),
            (ToolStage::Sampling, Some(2), Some(1)),
        ]
    );
}

#[test]
fn every_preview_goes_out_as_a_tool_preview_even_when_its_step_is_held_back() {
    let (tx, mut rx) = mpsc::channel(64);
    let (clock, _ms) = hand_clock();
    let sink = RateLimitedSink::new("c1".into(), tx, clock);

    for step in 1..=3 {
        sink.progress(ToolProgressUpdate {
            preview: Some(PreviewFrame::png(step, 20, "iVBO")),
            ..sampling(1, step, 20)
        });
    }

    let events = drain(&mut rx);
    assert_eq!(progress(&events).len(), 1);
    let steps: Vec<u32> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::ToolPreview {
                tool_call_id,
                frame,
            } if tool_call_id == "c1" => Some(frame.step),
            _ => None,
        })
        .collect();
    assert_eq!(steps, [1, 2, 3]);
}

#[test]
fn a_full_channel_drops_the_event_and_never_blocks() {
    let (tx, mut rx) = mpsc::channel(1);
    let (clock, _ms) = hand_clock();
    let sink = RateLimitedSink::new("c1".into(), tx, clock);

    sink.progress(ToolProgressUpdate {
        preview: Some(PreviewFrame::png(1, 20, "iVBO")),
        ..sampling(1, 1, 20)
    });

    let events = drain(&mut rx);
    assert_eq!(events.len(), 1, "the progress fit; the preview was dropped");
    assert!(matches!(events[0], AgentEvent::ToolProgress { .. }));
}
