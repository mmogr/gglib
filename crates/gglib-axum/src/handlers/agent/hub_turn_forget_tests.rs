//! A device's run on a hub chat is the chat's: the hub reads it, and
//! forgetting the device leaves it going and its reply saved.

use std::time::Duration;

use gglib_core::domain::runs::RunStatus;
use gglib_core::ports::{RunScope, RunsPort as _};

use crate::handlers::agent::compose::take_permit;
use crate::handlers::agent::launch::launch;
use crate::handlers::agent::run_fixture::{
    End, LOCAL, conversation, drain, finished_reply, logged, paced, saved, saving, settled, state,
};

/// The hub reads a device's run on its chat, and forgetting the device
/// mid-run leaves the run going and its reply saved: the chat is the hub's.
#[tokio::test]
async fn forgetting_the_device_mid_turn_still_saves_the_reply() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (p, _) = paced(finished_reply(), End::Finish, Duration::from_millis(20));
    launch(
        &state,
        "d1",
        RunScope::Device("phone".to_owned()),
        saving(id),
        p,
        take_permit(&state).unwrap(),
    )
    .await
    .unwrap();
    logged(&state, "d1", 1).await;
    let reading = state.runs.events(&LOCAL, "d1", 0).unwrap();

    assert_eq!(state.runs.forget_device("phone"), 0);
    let (frames, end) = drain(reading).await;
    assert_eq!(frames, finished_reply().len());
    assert_eq!(end.map(|i| i.status), Some(RunStatus::Completed));
    settled(&state).await;
    let rows = saved(&state, id).await;
    assert_eq!(
        rows.last().map(|r| r.content.as_str()),
        Some("ANSWER-SECRET")
    );
}
