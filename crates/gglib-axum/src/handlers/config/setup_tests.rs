//! The update route's step from the preflight to the pipeline, with a
//! stand-in for each: no install is looked at and nothing is pulled.
//!
//! `gglib config llama update` prints a plan's caution before it asks, and
//! its suite in `gglib-cli` compares that with `LOCAL_CHANGES_CAUTION`. The
//! stream's first event is compared with the same text here. The refusals are
//! compared over the route itself, in `tests/llama_update_preflight.rs`.

use std::path::PathBuf;

use gglib_runtime::llama::{Acceleration, BuildPhase, LOCAL_CHANGES_CAUTION, UpdateRefusal};
use serde_json::{Value, json};

use super::*;

/// The plan a preflight makes for a build from source, saying `caution`.
fn plan(caution: Option<&str>) -> UpdatePlan {
    UpdatePlan {
        acceleration: Acceleration::Cpu,
        llama_dir: PathBuf::from("/data/.llama/llama.cpp"),
        server_path: PathBuf::from("/data/.llama/bin/llama-server"),
        recorded: None,
        caution: caution.map(str::to_owned),
    }
}

/// Take the step on a preflight that answers `verdict`, with an update that
/// only reports the phase it would start with. Returns the events the stream
/// carried, each as the JSON a browser is sent, and how the step ended.
async fn step_on(verdict: anyhow::Result<UpdatePlan>) -> (Vec<Value>, anyhow::Result<()>) {
    let (tx, mut rx) = mpsc::channel(8);

    let ended = preflight_then_update(
        move || verdict,
        |_plan, tx| async move {
            let phase = BuildPhase::CloneOrUpdateRepo;
            let _ = tx.send(BuildEvent::PhaseStarted { phase }).await;
            Ok(())
        },
        tx,
    )
    .await;

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(serde_json::to_value(&event).expect("an event is JSON"));
    }
    (events, ended)
}

fn the_update_starting() -> Value {
    json!({ "type": "phase_started", "phase": "clone_or_update_repo" })
}

/// The words are the command's: both are `LOCAL_CHANGES_CAUTION`.
#[tokio::test]
async fn a_checkout_with_local_changes_is_cautioned_before_the_update_starts() {
    let (events, ended) = step_on(Ok(plan(Some(LOCAL_CHANGES_CAUTION)))).await;

    assert!(ended.is_ok(), "{ended:?}");
    assert_eq!(
        events,
        [
            json!({ "type": "log", "message": LOCAL_CHANGES_CAUTION }),
            the_update_starting(),
        ]
    );
}

#[tokio::test]
async fn a_clean_checkout_is_told_nothing_before_the_update_starts() {
    let (events, ended) = step_on(Ok(plan(None))).await;

    assert!(ended.is_ok(), "{ended:?}");
    assert_eq!(events, [the_update_starting()]);
}

#[tokio::test]
async fn a_refused_update_ends_in_the_refusal_and_starts_nothing() {
    let (events, ended) = step_on(Err(UpdateRefusal::NoSourceCheckout.into())).await;

    assert_eq!(
        ended.expect_err("a refusal").to_string(),
        UpdateRefusal::NoSourceCheckout.to_string()
    );
    assert!(events.is_empty(), "{events:?}");
}
