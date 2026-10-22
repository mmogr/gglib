//! A render nobody reads any more, against the scripted job and the real
//! admission queue of `job_tests.rs`: a job `sd-server` can cancel is
//! cancelled and nothing is killed; one that is generating is retired at
//! once, its server killed while the render still holds the slot and its
//! lease, then the slot emptied, then the turn ended, and whoever waited for
//! the GPU goes.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::{
    Host, SDXL_ID, SLOT, Timeline, drain, driver, flux_like, models, png, resident, sdxl_request,
    served_at,
};
use crate::sd::server::SdImageDriver;
use crate::sd::server::job_api::{CancelOutcome, JobState};
use crate::sd::server::job_poll::CANCEL_WAIT;
use gglib_core::ports::{
    GateError, GenerationTurn, ImageBatch, ImageError, ImageGenerationPort, ImageProgress,
    ImageStage,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

type Drawn = Result<ImageBatch, ImageError>;

/// What the stand-in manager logs for a render retired in order: the kill
/// saw the image model in its slot with the render's lease still counted,
/// and the slot was empty afterwards.
const KILLED_THEN_EMPTIED: [&str; 2] = ["kill, slot Some((1, 1))", "retired, slot None"];

/// A request for a picture, running in a task a test can abort as a reader
/// that leaves, and its reports.
fn draw(driver: &Arc<SdImageDriver>) -> (JoinHandle<Drawn>, mpsc::Receiver<ImageProgress>) {
    let (tx, rx) = mpsc::channel(64);
    let driver = Arc::clone(driver);
    let task = tokio::spawn(async move { driver.generate(sdxl_request(), tx).await });
    (task, rx)
}

/// The reader leaves: its request is dropped where it stands.
async fn leave(render: JoinHandle<Drawn>) {
    render.abort();
    assert!(render.await.unwrap_err().is_cancelled());
}

/// A chat that asks for its turn to generate, in a task of its own.
fn chat(host: &Arc<Host>) -> JoinHandle<Result<GenerationTurn, GateError>> {
    let gate = host.queue.generation_gate();
    tokio::spawn(async move { gate.llm_turn(None).await })
}

/// The turn of a chat that must have been given one by now. Asked, not
/// awaited: a chat still waiting fails the test instead of hanging it.
async fn granted(chat: JoinHandle<Result<GenerationTurn, GateError>>) -> GenerationTurn {
    assert!(chat.is_finished(), "the chat still waits for its turn");
    chat.await.unwrap().expect("the chat is given its turn")
}

fn log(host: &Host) -> Vec<String> {
    host.log.lock().unwrap().clone()
}

fn cancels(jobs: &Timeline) -> usize {
    jobs.cancels.load(Ordering::SeqCst)
}

/// A reader that leaves while the job generates: `sd-server` refuses the
/// cancel (409), so the server is killed while the render still holds the
/// slot and its lease, the slot is emptied, and the chat that waited behind
/// the render has its turn a second later. Nothing follows the job
/// afterwards.
#[tokio::test(start_paused = true)]
async fn a_reader_that_leaves_mid_render_stops_the_server_and_frees_the_gpu() {
    let jobs = Arc::new(Timeline::new(flux_like()).cancelled_with(CancelOutcome::Running));
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (render, _rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(40)).await;
    let chat = chat(&host);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(!chat.is_finished(), "the chat waits behind the render");
    assert!(log(&host).is_empty());

    leave(render).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let turn = granted(chat).await;

    assert_eq!(cancels(&jobs), 1, "the cancel was asked, once");
    assert_eq!(log(&host), KILLED_THEN_EMPTIED);
    assert_eq!(host.inflight(), None, "the slot is empty");
    assert!(
        !host.queue.ask_render_stop(SDXL_ID),
        "no render holds the model"
    );
    drop(turn);

    // 43 s in, with the job due to end at 72 s: nothing reads it to its end.
    tokio::time::sleep(Duration::from_mins(31)).await;
    assert_eq!(cancels(&jobs), 1);
    assert_eq!(log(&host), KILLED_THEN_EMPTIED, "retired once");
}

/// A reader that leaves while the job still waits inside `sd-server`: the
/// cancel is taken (200), the lease and the turn go, and nothing is killed:
/// the image model stays loaded.
#[tokio::test(start_paused = true)]
async fn a_reader_that_leaves_a_job_still_queued_cancels_it_and_kills_nothing() {
    let jobs = Arc::new(
        Timeline::new(vec![
            (0, JobState::Queued),
            (600, JobState::Generating(None)),
        ])
        .cancelled_with(CancelOutcome::Over),
    );
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (render, _rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(3)).await;
    let chat = chat(&host);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(!chat.is_finished(), "the chat waits behind the render");

    leave(render).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    drop(granted(chat).await);

    assert_eq!(cancels(&jobs), 1);
    assert!(log(&host).is_empty(), "nothing was killed");
    assert_eq!(host.inflight(), Some(0), "loaded still, with no request");
    tokio::time::sleep(Duration::from_mins(31)).await;
    assert!(log(&host).is_empty(), "nor is anything killed later");
    assert_eq!(host.inflight(), Some(0));
}

/// A job `sd-server` no longer knows (404) is over too: nothing is killed.
#[tokio::test(start_paused = true)]
async fn a_reader_that_leaves_a_job_sd_server_forgot_kills_nothing() {
    let jobs = Arc::new(Timeline::new(flux_like()).cancelled_with(CancelOutcome::Gone));
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (render, _rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(40)).await;
    leave(render).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(cancels(&jobs), 1);
    assert!(log(&host).is_empty(), "nothing was killed");
    assert_eq!(host.inflight(), Some(0));
}

/// A reader that leaves while the job is still being submitted leaves a job
/// `sd-server` may already have taken. The submission is finished in its own
/// task, which holds the turn and the lease meanwhile; then the job is asked
/// to cancel, and a generating one is retired. No turn and no lease is left.
#[tokio::test(start_paused = true)]
async fn a_reader_that_leaves_during_the_submission_ends_in_a_retired_render() {
    let jobs = Arc::new(Timeline {
        submit_takes: Duration::from_secs(5),
        ..Timeline::new(flux_like()).cancelled_with(CancelOutcome::Running)
    });
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (render, _rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(jobs.bodies.lock().unwrap().len(), 1, "the job was sent");
    let chat = chat(&host);

    leave(render).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(cancels(&jobs), 0, "the submission is not answered yet");
    assert_eq!(host.inflight(), Some(1), "the lease is held meanwhile");
    assert!(!chat.is_finished(), "and so is the turn");
    assert!(log(&host).is_empty());

    tokio::time::sleep(Duration::from_secs(3)).await;
    drop(granted(chat).await);
    assert_eq!(cancels(&jobs), 1, "the cancel was asked");
    assert_eq!(log(&host), KILLED_THEN_EMPTIED);
    assert_eq!(host.inflight(), None, "the slot is empty");
}

/// The same leave, when `sd-server` had not started the job by the time the
/// submission was answered: cancelled, nothing killed, nothing held.
#[tokio::test(start_paused = true)]
async fn a_reader_that_leaves_during_the_submission_of_a_queued_job_kills_nothing() {
    let jobs = Arc::new(Timeline {
        submit_takes: Duration::from_secs(5),
        ..Timeline::new(flux_like()).cancelled_with(CancelOutcome::Over)
    });
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (render, _rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let chat = chat(&host);
    leave(render).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    drop(granted(chat).await);
    assert_eq!(cancels(&jobs), 1);
    assert!(log(&host).is_empty(), "nothing was killed");
    assert_eq!(host.inflight(), Some(0), "the lease went");
}

/// A reader that is still there when the job completes has its picture, and
/// leaving afterwards changes nothing: no cancel is asked, nothing is
/// killed, and the model stays loaded, even on an `sd-server` that would
/// answer a cancel with 409.
#[tokio::test(start_paused = true)]
async fn a_reader_that_leaves_after_the_job_completed_changes_nothing() {
    let jobs = Arc::new(Timeline::new(flux_like()).cancelled_with(CancelOutcome::Running));
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (render, mut rx) = draw(&driver);
    let batch = render.await.unwrap().expect("drawn");
    assert_eq!(batch.images.len(), 1);
    assert_eq!(
        drain(&mut rx).last().map(|p| p.stage.clone()),
        Some(ImageStage::Finishing)
    );
    drop((batch, rx, driver));

    tokio::time::sleep(Duration::from_mins(31)).await;
    assert_eq!(cancels(&jobs), 0, "no cancel was asked");
    assert!(log(&host).is_empty(), "nothing was killed");
    assert_eq!(host.inflight(), Some(0), "loaded still, with no request");
}

/// A reader that leaves in the second between the job's end and the
/// driver's next read of it: `sd-server` answers the cancel of a finished
/// job with 200, so nothing is killed.
#[tokio::test(start_paused = true)]
async fn a_reader_that_leaves_before_the_finished_job_was_read_kills_nothing() {
    let jobs = Arc::new(Timeline::new(vec![
        (0, JobState::Generating(None)),
        (10, JobState::Completed(vec![png(1024, 1024)])),
    ]));
    let host = Host::new();
    let timing = super::JobTiming {
        poll: Duration::from_secs(4),
        ..super::JobTiming::default()
    };
    let driver = Arc::new(super::driver_timed(models(), &jobs, &host, timing));
    let (render, _rx) = draw(&driver);
    // Read at 4 s and 8 s; the job ends at 10 s; the next read is at 12 s.
    tokio::time::sleep(Duration::from_secs(11)).await;
    leave(render).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(cancels(&jobs), 1);
    assert!(log(&host).is_empty(), "nothing was killed");
    assert_eq!(host.inflight(), Some(0));
}

/// Everything that can leave a render leaving at once (the request, its
/// reports' reader and the driver itself) retires it once.
#[tokio::test(start_paused = true)]
async fn a_render_left_twice_is_retired_once() {
    let jobs = Arc::new(Timeline::new(flux_like()).cancelled_with(CancelOutcome::Running));
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (render, rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(40)).await;

    drop(rx);
    render.abort();
    render.abort();
    let _ = render.await;
    drop(driver);

    tokio::time::sleep(Duration::from_mins(31)).await;
    assert_eq!(cancels(&jobs), 1, "one cancel");
    assert_eq!(log(&host), KILLED_THEN_EMPTIED, "one kill, one eviction");
}

/// A render its driver already retired for a stall has nothing left to
/// stop when its request is then dropped: no cancel, no second kill.
#[tokio::test(start_paused = true)]
async fn a_render_retired_for_a_stall_is_not_retired_again_when_dropped() {
    let jobs = Arc::new(
        Timeline::new(vec![(0, JobState::Generating(None))]).cancelled_with(CancelOutcome::Running),
    );
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (render, _rx) = draw(&driver);
    let refused = render.await.unwrap().unwrap_err();
    assert!(matches!(refused, ImageError::Stalled { .. }), "{refused:?}");
    tokio::time::sleep(Duration::from_mins(31)).await;
    assert_eq!(cancels(&jobs), 0);
    assert_eq!(log(&host), KILLED_THEN_EMPTIED);
}

/// The slot changed hands while the cancel was on its way: a Stop emptied
/// it and another model was launched there with a request in flight. The
/// retire that follows the refused cancel empties nothing and takes no
/// request from the newcomer.
#[tokio::test(start_paused = true)]
async fn a_late_retire_spares_a_newcomer_in_the_slot() {
    let jobs = Arc::new(Timeline {
        cancel_takes: Duration::from_secs(2),
        ..Timeline::new(flux_like()).cancelled_with(CancelOutcome::Running)
    });
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (render, _rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(40)).await;
    leave(render).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(cancels(&jobs), 1, "the cancel is on its way");
    assert!(log(&host).is_empty());

    assert!(host.queue.evict(SLOT).is_some());
    let newcomer = host.queue.install(SLOT, resident(7, "embedder"));

    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        log(&host),
        ["kill, slot Some((7, 1))", "retired, slot Some(7)"],
        "the newcomer was in the slot at the kill, and is after it"
    );
    assert_eq!(host.inflight(), Some(1), "the newcomer keeps its request");
    tokio::time::sleep(Duration::from_mins(31)).await;
    assert_eq!(host.inflight(), Some(1));
    drop(newcomer);
}

/// After a render was stopped the image model is not loaded, and the next
/// request for a picture loads it through the ordinary admission and draws.
#[tokio::test(start_paused = true)]
async fn the_next_draw_after_a_stopped_render_loads_the_model_again() {
    let jobs = Arc::new(Timeline::new(flux_like()).cancelled_with(CancelOutcome::Running));
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (render, _rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(40)).await;
    leave(render).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(host.inflight(), None, "the image model is not loaded");
    assert_eq!(host.admits.load(Ordering::SeqCst), 1);

    let (again, _rx) = draw(&driver);
    let batch = again.await.unwrap().expect("drawn");

    assert_eq!(batch.images.len(), 1);
    assert_eq!(host.admits.load(Ordering::SeqCst), 2, "admitted again");
    assert_eq!(
        host.queue.slot(SLOT).map(|r| r.model_id),
        Some(SDXL_ID),
        "loaded again"
    );
    assert_eq!(host.inflight(), Some(0));
    assert_eq!(jobs.bodies.lock().unwrap().len(), 2, "one job each");
    assert_eq!(log(&host), KILLED_THEN_EMPTIED, "only the first was killed");
}

/// A draw admitted while the render ahead of it drew holds a lease on the
/// model that render's reader then retired: when its turn comes it is
/// admitted again, loads the model afresh and draws, and nothing is sent to
/// the server that was stopped.
#[tokio::test(start_paused = true)]
async fn a_draw_waiting_behind_a_stopped_render_is_admitted_again() {
    let jobs = Arc::new(Timeline::new(flux_like()).cancelled_with(CancelOutcome::Running));
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (first, _rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(40)).await;
    let (second, _second_rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(host.inflight(), Some(2), "both hold a lease");

    leave(first).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(host.admits.load(Ordering::SeqCst), 3, "admitted again");
    assert_eq!(host.inflight(), Some(1), "loaded again, with its request");

    let batch = second.await.unwrap().expect("drawn");
    assert_eq!(batch.images.len(), 1);
    assert_eq!(
        host.queue.slot(SLOT).map(|r| r.model_id),
        Some(SDXL_ID),
        "loaded again"
    );
    assert_eq!(host.inflight(), Some(0));
    assert_eq!(
        *jobs.sent_to.lock().unwrap(),
        [
            served_at(0, SDXL_ID).base_url,
            served_at(2, SDXL_ID).base_url
        ],
        "one job each, the second through its new admission"
    );
    assert_eq!(
        log(&host),
        ["kill, slot Some((1, 2))", "retired, slot None"],
        "only the first was killed"
    );
}

/// The same draw, when another model took the emptied slot before its turn
/// came: its first lease was settled with the slot, so admitting it again
/// takes no request from the newcomer.
#[tokio::test(start_paused = true)]
async fn a_draw_admitted_again_takes_no_request_from_a_newcomer() {
    let jobs = Arc::new(Timeline::new(flux_like()).cancelled_with(CancelOutcome::Running));
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (first, _rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(40)).await;
    let chat = chat(&host);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let (second, _second_rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(1)).await;

    leave(first).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let turn = granted(chat).await;
    assert_eq!(host.inflight(), None, "the slot is empty");
    let newcomer = host.queue.install(SLOT, resident(7, "embedder"));
    drop(turn);
    tokio::time::sleep(Duration::from_secs(1)).await;

    assert_eq!(host.admits.load(Ordering::SeqCst), 3, "admitted again");
    assert_eq!(
        host.inflight(),
        Some(2),
        "the newcomer keeps its request, and the draw adds its own"
    );
    drop(newcomer);
    second.await.unwrap().expect("drawn");
    assert_eq!(host.inflight(), Some(0));
}

/// A request that leaves while it only waits in the gate's line, behind
/// another render, leaves the line: no job of its own was submitted, no
/// cancel is asked, nothing is killed, and the render ahead of it finishes.
#[tokio::test(start_paused = true)]
async fn a_reader_that_leaves_the_gates_line_kills_nothing() {
    let jobs = Arc::new(Timeline::new(flux_like()).cancelled_with(CancelOutcome::Running));
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let (first, _rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(10)).await;
    let (second, mut second_rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(host.inflight(), Some(2), "both hold a lease");
    assert!(
        drain(&mut second_rx)
            .iter()
            .any(|p| matches!(p.stage, ImageStage::Queued { .. })),
        "the second waits in line"
    );

    leave(second).await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    assert_eq!(
        host.inflight(),
        Some(1),
        "its lease went, the first's stays"
    );
    assert_eq!(cancels(&jobs), 0, "it had no job to cancel");
    assert!(log(&host).is_empty(), "nothing was killed");
    assert_eq!(
        jobs.bodies.lock().unwrap().len(),
        1,
        "one job was submitted"
    );
    first
        .await
        .unwrap()
        .expect("the render ahead of it is drawn");
    assert!(log(&host).is_empty());
}

/// The same behind a chat: a request waiting for the chat's turn to end
/// leaves the line, and the chat's turn and the loaded model are untouched.
#[tokio::test(start_paused = true)]
async fn a_reader_that_leaves_the_line_behind_a_chat_kills_nothing() {
    let jobs = Arc::new(Timeline::new(flux_like()).cancelled_with(CancelOutcome::Running));
    let host = Host::new();
    let driver = Arc::new(driver(models(), &jobs, &host));
    let chat = chat(&host);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let turn = granted(chat).await;
    let (render, _rx) = draw(&driver);
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(
        host.inflight(),
        Some(1),
        "admitted, and waiting for its turn"
    );

    leave(render).await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    assert_eq!(host.inflight(), Some(0), "loaded still, with no request");
    assert!(
        jobs.bodies.lock().unwrap().is_empty(),
        "nothing was submitted"
    );
    assert_eq!(cancels(&jobs), 0);
    assert!(log(&host).is_empty(), "nothing was killed");
    drop(turn);
}

/// A cancel `sd-server` does not answer is waited for [`CANCEL_WAIT`] and
/// no longer, and one that fails is not waited for at all: either way the
/// job is taken to be generating and the render is retired. A person's Stop
/// on the model in that wait finds a render to ask, and none once it is
/// retired.
#[tokio::test(start_paused = true)]
async fn a_cancel_that_is_not_answered_or_fails_retires_the_render() {
    let silent = Timeline {
        cancel_takes: Duration::from_mins(10),
        ..Timeline::new(flux_like())
    };
    let failing = Timeline {
        cancel_fails: Some("connection reset".to_owned()),
        ..Timeline::new(flux_like())
    };
    for (jobs, waits) in [(silent, CANCEL_WAIT), (failing, Duration::ZERO)] {
        let jobs = Arc::new(jobs);
        let host = Host::new();
        let driver = Arc::new(driver(models(), &jobs, &host));
        let (render, _rx) = draw(&driver);
        tokio::time::sleep(Duration::from_secs(40)).await;
        leave(render).await;

        if !waits.is_zero() {
            tokio::time::sleep(waits.saturating_sub(Duration::from_millis(100))).await;
            assert!(log(&host).is_empty(), "still waiting for the answer");
            assert!(host.queue.ask_render_stop(SDXL_ID), "a render holds it");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(cancels(&jobs), 1);
        assert_eq!(log(&host), KILLED_THEN_EMPTIED);
        assert!(!host.queue.ask_render_stop(SDXL_ID), "no render is left");
    }
}
