//! A load's progress on the engine's watch, and a shutdown that stops a load
//! under way or lands as one ends.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gglib_core::ports::{LaunchOverrides, ModelRuntimeError, ModelRuntimePort};

use super::test_support::{GIB, Shelf, hello, read, runtime, slow, until};
use crate::state::EngineState;

#[tokio::test]
async fn a_load_is_loading_at_each_fraction_it_reports_then_ready() {
    let mut script = slow(Duration::ZERO, Duration::from_millis(40));
    script.load_steps = 4;
    let shelf = Shelf::new();
    let (runtime, _) = runtime(script, vec![shelf.model(1, "small", GIB)]).await;

    runtime
        .admit("small", None, None, LaunchOverrides::default())
        .await
        .expect("the model loads");

    let loading = |fraction| EngineState::Loading {
        model: "small".to_owned(),
        fraction,
    };
    let ready = EngineState::Ready {
        model: "small".to_owned(),
        n_ctx: 8192,
    };
    assert_eq!(
        runtime.inner.engine.states().sent(),
        vec![
            loading(0.0),
            loading(0.25),
            loading(0.5),
            loading(0.75),
            loading(1.0),
            ready
        ]
    );
}

#[tokio::test]
async fn shutdown_during_a_load_stops_it_and_ends_stopped() {
    // Ten seconds of load in a thousand steps, which only the shutdown ends.
    let mut script = slow(Duration::ZERO, Duration::from_secs(10));
    script.load_steps = 1000;
    let shelf = Shelf::new();
    let (runtime, record) = runtime(script, vec![shelf.model(1, "small", GIB)]).await;
    let mut states = runtime.subscribe();
    let admitting = Arc::clone(&runtime);
    let admission = tokio::spawn(async move {
        admitting
            .admit("small", None, None, LaunchOverrides::default())
            .await
    });
    until(
        &mut states,
        |s| matches!(s, EngineState::Loading { fraction, .. } if *fraction > 0.0),
    )
    .await;

    let started = Instant::now();
    runtime.shutdown().await.expect("the shim stops");

    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the shutdown waited {:?} for the load",
        started.elapsed()
    );
    let refused = admission.await.expect("the admission ends");
    assert!(
        matches!(refused, Err(ModelRuntimeError::Internal(_))),
        "{refused:?}"
    );
    assert_eq!(runtime.state(), EngineState::Stopped);
    let record = read(&record);
    assert_eq!(record.loads.len(), 1);
    assert_eq!(record.loads_aborted, 1, "the load was stopped");
    assert_eq!(record.unloads, 0, "the load stopped held no model");
    assert!(runtime.current_model().await.is_none());
    // The watch ends on Stopped.
    until(&mut states, |s| *s == EngineState::Stopped).await;
    assert!(states.changed().await.is_err(), "the watch has ended");
}

#[tokio::test]
async fn a_shutdown_landing_as_a_load_ends_frees_the_model_it_loaded() {
    // The load raises the runtime's closing abort past its last check, as a
    // shutdown does that lands between the load's end and the admission's.
    let mut script = hello();
    script.load_aborted_late = true;
    let shelf = Shelf::new();
    let (runtime, record) = runtime(script, vec![shelf.model(1, "small", GIB)]).await;

    let refused = runtime
        .admit("small", None, None, LaunchOverrides::default())
        .await;

    assert!(
        matches!(refused, Err(ModelRuntimeError::Internal(_))),
        "{refused:?}"
    );
    let record = read(&record);
    assert_eq!(record.loads_aborted, 0, "the load ended holding the model");
    assert_eq!(record.unloads, 1, "the model it loaded is freed");
    assert!(runtime.current_model().await.is_none());
    assert_eq!(runtime.state(), EngineState::Idle);
    runtime.shutdown().await.expect("the shim stops");
    assert_eq!(runtime.state(), EngineState::Stopped);
}
