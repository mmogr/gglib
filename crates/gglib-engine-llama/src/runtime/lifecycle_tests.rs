//! The engine's states through replies, swaps, stops, failed loads and a
//! runtime shut down.

use std::time::Duration;

use futures_util::StreamExt as _;
use gglib_core::ports::{LaunchOverrides, ModelRuntimeError, ModelRuntimePort};
use serde_json::{Value, json};

use super::test_support::{GIB, Shelf, hello, read, runtime, slow, until};
use crate::decoder::{FinishReason, Script};
use crate::state::EngineState;

const STEP: Duration = Duration::from_millis(100);

#[tokio::test]
async fn a_runtime_shut_down_admits_nothing_and_its_shim_is_gone() {
    let shelf = Shelf::new();
    let (runtime, _) = runtime(hello(), vec![shelf.model(1, "small", GIB)]).await;
    let target = runtime
        .admit("small", None, None, LaunchOverrides::default())
        .await
        .unwrap()
        .into_target();

    runtime.shutdown().await.unwrap();
    runtime
        .shutdown()
        .await
        .expect("a second shutdown is a no-op");

    let refused = runtime
        .admit("small", None, None, LaunchOverrides::default())
        .await;
    assert!(refused.is_err(), "{refused:?}");
    assert!(
        tokio::net::TcpStream::connect(("127.0.0.1", target.port))
            .await
            .is_err(),
        "the shim no longer listens"
    );
    let mut late = runtime.subscribe();
    assert_eq!(*late.borrow(), EngineState::Stopped);
    assert!(late.changed().await.is_err());
}

#[tokio::test]
async fn stopping_the_model_frees_it_and_the_next_admission_loads_it_again() {
    let shelf = Shelf::new();
    let (runtime, record) = runtime(hello(), vec![shelf.model(4, "small", GIB)]).await;
    let admit = || runtime.admit("small", None, None, LaunchOverrides::default());
    admit().await.unwrap();

    assert!(!runtime.stop_model(5).await.unwrap(), "not the model held");
    assert!(runtime.current_model().await.is_some());
    assert!(runtime.stop_model(4).await.unwrap());
    assert!(runtime.current_model().await.is_none());
    assert_eq!(runtime.state(), EngineState::Idle);
    assert_eq!(read(&record).unloads, 1);

    assert!(admit().await.unwrap().target.just_started);
    runtime.stop_current().await.unwrap();
    assert_eq!(runtime.state(), EngineState::Idle);
    assert_eq!(read(&record).loads.len(), 2);
    assert_eq!(read(&record).unloads, 2);
}

#[tokio::test]
async fn a_reply_is_generating_until_it_ends() {
    let shelf = Shelf::new();
    let models = vec![shelf.model(1, "small", GIB)];
    let (runtime, _) = runtime(slow(STEP, Duration::ZERO), models).await;
    let target = runtime
        .admit("small", None, None, LaunchOverrides::default())
        .await
        .unwrap()
        .into_target();
    let mut states = runtime.subscribe();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let body = json!({"model": "small", "messages": [{"role": "user", "content": "hi"}]});
    let reply = tokio::spawn(
        client
            .post(format!("{}/v1/chat/completions", target.base_url))
            .json(&body)
            .send(),
    );

    let generating = until(&mut states, |s| matches!(s, EngineState::Generating { .. })).await;
    assert_eq!(
        generating,
        EngineState::Generating {
            id: 1,
            model: "small".to_owned(),
            n_ctx: 8192
        }
    );
    let answered = reply.await.unwrap().expect("the shim answers");
    assert_eq!(answered.status(), 200);
    let ready = until(&mut states, |s| matches!(s, EngineState::Ready { .. })).await;
    assert_eq!(
        ready,
        EngineState::Ready {
            id: 1,
            model: "small".to_owned(),
            n_ctx: 8192
        }
    );
}

#[tokio::test]
async fn a_load_that_fails_is_failed_and_the_next_admission_tries_again() {
    let mut script = hello();
    script.chat_template = "{% if %}".to_owned();
    let shelf = Shelf::new();
    let (runtime, record) = runtime(script, vec![shelf.model(1, "broken", GIB)]).await;
    let admit = || runtime.admit("broken", None, None, LaunchOverrides::default());

    let failed = admit().await;
    assert!(
        matches!(&failed, Err(ModelRuntimeError::SpawnFailed(_))),
        "{failed:?}"
    );
    assert!(
        matches!(runtime.state(), EngineState::Failed { id: 1, ref model, .. } if model == "broken"),
        "{:?}",
        runtime.state()
    );
    assert!(runtime.current_model().await.is_none());

    assert!(admit().await.is_err());
    assert_eq!(
        read(&record).loads.len(),
        2,
        "the second admission loaded again"
    );
}

#[tokio::test]
async fn a_load_that_panics_is_failed_and_the_next_admission_loads() {
    let mut script = hello();
    script.load_panics = true;
    let shelf = Shelf::new();
    let (runtime, record) = runtime(script, vec![shelf.model(1, "fragile", GIB)]).await;
    let mut states = runtime.subscribe();
    let admit = || runtime.admit("fragile", None, None, LaunchOverrides::default());

    let panicked = admit().await;
    assert!(
        matches!(&panicked, Err(ModelRuntimeError::Internal(_))),
        "{panicked:?}"
    );
    let failed = until(&mut states, |s| matches!(s, EngineState::Failed { .. })).await;
    assert!(
        matches!(&failed, EngineState::Failed { id: 1, model, .. } if model == "fragile"),
        "{failed:?}"
    );
    assert!(runtime.current_model().await.is_none());

    let admitted = admit().await.expect("the next load does not panic");
    assert!(admitted.target.just_started);
    assert_eq!(read(&record).loads.len(), 2);
    assert_eq!(
        runtime.state(),
        EngineState::Ready {
            id: 1,
            model: "fragile".to_owned(),
            n_ctx: 8192
        }
    );
}

#[tokio::test]
async fn a_model_swapped_out_mid_reply_ends_the_reply_with_an_error() {
    // Sixteen seconds of reply, which only the swap ends.
    let mut script = Script::new(vec!["tick "; 8000]);
    script.step_delay = Duration::from_millis(2);
    let shelf = Shelf::new();
    let models = vec![shelf.model(1, "first", GIB), shelf.model(2, "second", GIB)];
    let (runtime, record) = runtime(script, models).await;
    let admit = |name| runtime.admit(name, None, None, LaunchOverrides::default());
    let target = admit("first").await.unwrap().into_target();
    let mut states = runtime.subscribe();
    let body = json!({
        "model": "first",
        "stream": true,
        "messages": [{"role": "user", "content": "Go on"}],
    });
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .post(format!("{}/v1/chat/completions", target.base_url))
        .json(&body)
        .send()
        .await
        .expect("the shim answers");
    let mut stream = response.bytes_stream();
    let mut seen = String::new();
    while !seen.contains("tick") {
        let chunk = stream.next().await.expect("more of the stream");
        seen.push_str(&String::from_utf8_lossy(&chunk.expect("bytes")));
    }
    until(&mut states, |s| matches!(s, EngineState::Generating { .. })).await;

    let swapped = admit("second").await.expect("the second model is admitted");
    while let Some(chunk) = stream.next().await {
        seen.push_str(&String::from_utf8_lossy(&chunk.expect("bytes")));
    }

    assert_eq!(swapped.target.model_name, "second");
    assert!(!seen.contains("[DONE]"), "{seen}");
    let last = seen.trim_end().rsplit("data: ").next().unwrap_or_default();
    let error: Value = serde_json::from_str(last).expect("the last frame is JSON");
    assert_eq!(error["error"]["type"], "unavailable_error", "{last}");
    let record = read(&record);
    assert_eq!(record.finished[0].reason, FinishReason::Aborted);
    assert_eq!(record.loads_over_a_model, 0, "never two models at once");
    let ready = EngineState::Ready {
        id: 2,
        model: "second".to_owned(),
        n_ctx: 8192,
    };
    assert_eq!(runtime.state(), ready);
    // Every state sent: the reply that ended late did not mark the model
    // swapped out ready again.
    let sent = runtime.inner.engine.states().sent();
    let unloading = EngineState::Unloading {
        id: 1,
        model: "first".to_owned(),
    };
    let from = sent
        .iter()
        .position(|s| *s == unloading)
        .unwrap_or_else(|| panic!("the first model was unloaded: {sent:?}"));
    assert!(
        !sent[from..]
            .iter()
            .any(|s| matches!(s, EngineState::Ready { id: 1, .. })),
        "{sent:?}"
    );
    assert_eq!(sent.last(), Some(&ready), "{sent:?}");
}
