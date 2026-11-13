//! A paired device's run waits in line for an agent slot when every slot
//! is taken: its `PUT` answers at once, the run says its place each second,
//! and it runs once the slot is its own. This machine's own turn is still
//! refused `agent_busy` at once.

use std::sync::Arc;
use std::time::Duration;

use gglib_core::domain::runs::RunStatus;
use gglib_core::ports::{RunEvent, RunScope, RunsPort as _};
use serde_json::{Value, json};

use super::hub_turn_tests::{chat, device, refused, turn};
use super::start;
use crate::handlers::agent::compose::take_permit;
use crate::handlers::agent::image_gate::image_gate_tests::model;
use crate::handlers::agent::run_fixture::{
    End, LOCAL, finished_reply, launch, paced, prepared, saved, saving, settled, state,
};
use crate::handlers::agent::slot_line::{AGENT_SLOT_LINE, AGENT_SLOT_WAIT, Slot, SlotLine};
use crate::state::AppState;

/// `state`, its line of `cap` waiters giving up after `bound`.
fn with_line(state: AppState, cap: usize, bound: Duration) -> AppState {
    let mut context = Arc::into_inner(state).expect("the only handle");
    let slots = Arc::clone(&context.agent_semaphore);
    context.agent_line = Arc::new(SlotLine::new(slots, cap, bound));
    Arc::new(context)
}

/// A place at the back of the line.
fn join(state: &AppState) -> Slot {
    let slot = state.agent_line.take_or_join().expect("room in line");
    assert!(matches!(slot, Slot::Waiting(_)), "every slot is taken");
    slot
}

/// The first frame of `scope`'s run `id` that `wanted` takes, from the
/// start of its log.
async fn frame(
    state: &AppState,
    scope: &RunScope,
    id: &str,
    wanted: impl Fn(&Value) -> bool + Send + Sync,
) -> Value {
    let mut events = state.runs.events(scope, id, 0).unwrap();
    let found = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = futures_util::StreamExt::next(&mut events).await {
            if let RunEvent::Frame { data, .. } = event {
                let frame: Value = serde_json::from_str(&data).unwrap();
                if wanted(&frame) {
                    return frame;
                }
            }
        }
        panic!("the run ended without the frame");
    });
    found.await.expect("the frame within five seconds")
}

/// A wait for a slot at `position`.
fn in_line(position: u32) -> impl Fn(&Value) -> bool {
    move |frame| frame["reason"] == "agent_slot" && frame["position"] == position
}

fn status(state: &AppState, scope: &RunScope, id: &str) -> RunStatus {
    state.runs.get(scope, id).unwrap().status
}

/// The bound is long enough for a run or two ahead to finish.
#[test]
fn the_line_waits_two_minutes() {
    assert_eq!(AGENT_SLOT_WAIT, Duration::from_mins(2));
}

/// The daemon's own line holds sixteen, each giving up two minutes after
/// it joined: the seventeenth is refused.
#[tokio::test]
async fn the_daemons_line_holds_sixteen_for_two_minutes_each() {
    assert_eq!(AGENT_SLOT_LINE, 16);
    let (_dir, state) = state().await;
    let _held = take_permit(&state).expect("the one slot");
    let line: Vec<Slot> = (0..16).map(|_| join(&state)).collect();
    for slot in &line {
        let Slot::Waiting(place) = slot else {
            unreachable!("`join` takes a place")
        };
        assert_eq!(place.deadline() - place.joined(), AGENT_SLOT_WAIT);
    }

    assert!(state.agent_line.take_or_join().is_none());
    drop(line);
    assert!(state.agent_line.take_or_join().is_some());
}

/// With the one slot held, a device's turn is accepted and waits, first in
/// line, `queued`, having written nothing; then, its slot free, it runs on,
/// to its model's load. This machine's own turn is refused at once.
#[tokio::test]
async fn a_devices_turn_waits_in_line_and_this_machines_is_refused() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let held = take_permit(&state).expect("the one slot");

    let refusal = refused(start(&state, LOCAL, "l1", turn(id, "now"), None).await);
    assert_eq!(refusal, (429, "agent_busy"));
    assert!(state.runs.existing(&LOCAL, "l1").unwrap().is_none());

    let phone = device("phone");
    let created = start(&state, phone.clone(), "d1", turn(id, "later"), None)
        .await
        .unwrap();
    assert!(created.created);
    let wait = frame(&state, &phone, "d1", in_line(1)).await;
    // Its seconds too, on a machine too slow to say so within the first.
    let said = wait["said"].as_str().unwrap();
    assert!(
        said.starts_with("Waiting for a free agent slot, 1 in line…"),
        "{said}"
    );
    let keys: Vec<&String> = wait.as_object().unwrap().keys().collect();
    let named = [
        "elapsed_ms",
        "position",
        "reason",
        "said",
        "step",
        "total",
        "type",
    ];
    assert_eq!(keys, named, "no model: {wait}");
    assert_eq!(status(&state, &phone, "d1"), RunStatus::Queued);
    assert_eq!(saved(&state, id).await.len(), 2, "nothing written");

    drop(held);
    // Its model cannot load here: past the slot, the run waits for that.
    frame(&state, &phone, "d1", |f| f["reason"] == "model_load").await;
    settled(&state).await;
}

/// A device's run in line runs once the slot frees, and its reply is saved.
#[tokio::test]
async fn a_waiting_run_replies_once_its_slot_frees() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let held = take_permit(&state).expect("the one slot");
    let (p, _) = prepared(finished_reply(), End::Finish);
    launch(&state, "d1", device("phone"), saving(id), p, join(&state)).unwrap();
    frame(&state, &device("phone"), "d1", in_line(1)).await;

    drop(held);
    settled(&state).await;

    assert_eq!(status(&state, &device("phone"), "d1"), RunStatus::Completed);
    let rows = saved(&state, id).await;
    assert_eq!(rows[2].content, "PROMPT-SECRET", "its message");
    assert_eq!(rows.last().unwrap().content, "ANSWER-SECRET", "its reply");
    assert_eq!(state.agent_semaphore.available_permits(), 1);
}

/// Two in line are first and second; the second is first once the first
/// has the slot, and never gets it before the first.
#[tokio::test]
async fn the_second_in_line_moves_up_once_the_first_has_the_slot() {
    let (_dir, state) = state().await;
    let held = take_permit(&state).expect("the one slot");
    let phone = device("phone");
    let (first, _) = paced(finished_reply(), End::Hang, Duration::from_millis(1));
    launch(
        &state,
        "d1",
        phone.clone(),
        saving(chat(&state, None).await),
        first,
        join(&state),
    )
    .unwrap();
    let (second, _) = prepared(finished_reply(), End::Finish);
    launch(
        &state,
        "d2",
        phone.clone(),
        saving(chat(&state, None).await),
        second,
        join(&state),
    )
    .unwrap();
    frame(&state, &phone, "d1", in_line(1)).await;
    frame(&state, &phone, "d2", in_line(2)).await;

    drop(held);
    frame(&state, &phone, "d2", in_line(1)).await;
    // The first's reply has begun: it is in progress from its first event.
    frame(&state, &phone, "d1", |f| f["type"] != "waiting").await;
    assert_eq!(status(&state, &phone, "d1"), RunStatus::InProgress);
    assert_eq!(status(&state, &phone, "d2"), RunStatus::Queued);

    state.runs.cancel(&phone, "d1").unwrap();
    settled(&state).await;
    assert_eq!(status(&state, &phone, "d2"), RunStatus::Completed);
}

/// A run cancelled while it waits ends `cancelled`, takes no slot, writes
/// nothing, and the run behind it moves up.
#[tokio::test]
async fn a_run_cancelled_in_line_leaves_it_and_the_next_moves_up() {
    let (_dir, state) = state().await;
    let _held = take_permit(&state).expect("the one slot");
    let phone = device("phone");
    let id = chat(&state, None).await;
    let (first, _) = prepared(finished_reply(), End::Finish);
    launch(&state, "d1", phone.clone(), saving(id), first, join(&state)).unwrap();
    let (second, _) = prepared(finished_reply(), End::Finish);
    launch(
        &state,
        "d2",
        phone.clone(),
        saving(chat(&state, None).await),
        second,
        join(&state),
    )
    .unwrap();
    frame(&state, &phone, "d2", in_line(2)).await;

    state.runs.cancel(&phone, "d1").unwrap();
    frame(&state, &phone, "d2", in_line(1)).await;

    assert_eq!(status(&state, &phone, "d1"), RunStatus::Cancelled);
    assert_eq!(saved(&state, id).await.len(), 2, "nothing written");
    assert_eq!(state.agent_semaphore.available_permits(), 0);
    state.runs.shutdown();
}

/// A waiting run says so again each second, with the time it has waited.
#[tokio::test]
async fn a_waiting_run_says_so_again_each_second() {
    let (_dir, state) = state().await;
    let _held = take_permit(&state).expect("the one slot");
    let id = chat(&state, None).await;
    let phone = device("phone");
    start(&state, phone.clone(), "d1", turn(id, "later"), None)
        .await
        .unwrap();

    let waited = |f: &Value| f["elapsed_ms"].as_u64().is_some_and(|ms| ms >= 1000);
    let again = frame(&state, &phone, "d1", waited).await;
    assert_eq!(again["reason"], "agent_slot");
    assert_eq!(again["position"], 1);
    let said = again["said"].as_str().unwrap();
    assert!(
        said.starts_with("Waiting for a free agent slot, 1 in line… ") && said.ends_with(" s"),
        "{said}"
    );
    state.runs.shutdown();
}

/// A forgotten device's phone-kept chat run leaves the line, and the run
/// behind it moves up. Its turn on a hub chat is the chat's: it keeps its
/// place.
#[tokio::test]
async fn a_forgotten_devices_chat_run_leaves_the_line() {
    let (_dir, state) = state().await;
    model(&state, "qwen-vision", true).await;
    let _held = take_permit(&state).expect("the one slot");
    let phone = device("phone");
    let body = json!({ "model": "qwen-vision", "messages": [{ "role": "user", "content": "hi" }] });
    let start_chat = crate::handlers::agent::chat_turn::start;
    start_chat(&state, "phone", "p1", body, false.into())
        .await
        .unwrap();
    let id = chat(&state, None).await;
    start(&state, phone.clone(), "d1", turn(id, "later"), None)
        .await
        .unwrap();
    frame(&state, &phone, "d1", in_line(2)).await;

    assert_eq!(state.runs.forget_device("phone"), 1);
    frame(&state, &phone, "d1", in_line(1)).await;

    assert!(state.runs.existing(&phone, "p1").unwrap().is_none());
    assert_eq!(status(&state, &phone, "d1"), RunStatus::Queued);
    assert_eq!(state.agent_semaphore.available_permits(), 0);
    state.runs.shutdown();
}

/// The daemon stopping ends a waiting run: it leaves the line, having taken
/// no slot and written nothing.
#[tokio::test]
async fn a_waiting_run_leaves_the_line_at_shutdown() {
    let (_dir, state) = state().await;
    let _held = take_permit(&state).expect("the one slot");
    let id = chat(&state, None).await;
    let phone = device("phone");
    start(&state, phone.clone(), "d1", turn(id, "later"), None)
        .await
        .unwrap();
    frame(&state, &phone, "d1", in_line(1)).await;

    state.runs.shutdown();
    settled(&state).await;

    let Slot::Waiting(place) = join(&state) else {
        unreachable!("`join` takes a place")
    };
    assert_eq!(place.position(), 1, "the line is empty again");
    assert_eq!(state.agent_semaphore.available_permits(), 0);
    assert_eq!(saved(&state, id).await.len(), 2, "nothing written");
}

/// A run that waits past the bound ends `failed` `agent_busy`, having
/// written nothing.
#[tokio::test]
async fn a_run_that_waits_past_the_bound_ends_agent_busy() {
    let (_dir, state) = state().await;
    let state = with_line(state, 16, Duration::from_millis(300));
    let _held = take_permit(&state).expect("the one slot");
    let id = chat(&state, None).await;
    let phone = device("phone");
    start(&state, phone.clone(), "d1", turn(id, "later"), None)
        .await
        .unwrap();
    settled(&state).await;

    let run = state.runs.get(&phone, "d1").unwrap();
    assert_eq!(run.status, RunStatus::Failed);
    let error = run.error.unwrap();
    assert_eq!(error.code, "agent_busy");
    assert_eq!(
        error.message,
        "all agent loop slots are in use; try again later"
    );
    assert_eq!(saved(&state, id).await.len(), 2, "nothing written");
}

/// With the line full, a device's run is refused `agent_busy` at once, and
/// nothing is reserved.
#[tokio::test]
async fn a_full_line_refuses_at_once() {
    let (_dir, state) = state().await;
    let state = with_line(state, 1, AGENT_SLOT_WAIT);
    let _held = take_permit(&state).expect("the one slot");
    let id = chat(&state, None).await;
    let phone = device("phone");
    start(&state, phone.clone(), "d1", turn(id, "first"), None)
        .await
        .unwrap();

    let other = chat(&state, None).await;
    let refusal = refused(start(&state, phone.clone(), "d2", turn(other, "second"), None).await);
    assert_eq!(refusal, (429, "agent_busy"));
    assert!(state.runs.existing(&phone, "d2").unwrap().is_none());
    assert_eq!(saved(&state, other).await.len(), 2, "nothing written");
    state.runs.shutdown();
}

/// A phone-kept chat run, the issue's own case, waits in line too.
#[tokio::test]
async fn a_devices_chat_run_waits_in_line() {
    let (_dir, state) = state().await;
    model(&state, "qwen-vision", true).await;
    let _held = take_permit(&state).expect("the one slot");
    let body = json!({ "model": "qwen-vision", "messages": [{ "role": "user", "content": "hi" }] });

    let start = crate::handlers::agent::chat_turn::start;
    let created = start(&state, "phone", "p1", body, false.into())
        .await
        .unwrap();

    assert!(created.created);
    frame(&state, &device("phone"), "p1", in_line(1)).await;
    state.runs.shutdown();
}
