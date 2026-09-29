//! One event that could never fit the log fails the run instead of growing
//! the executor's buffer. The limit is passed in, so no test sends 8 MB.

use std::sync::Arc;
use std::time::Duration;

use gglib_core::domain::runs::RunStatus;
use gglib_core::ports::{RunScope, RunsPort};
use serde_json::json;

use super::RunRegistry;
use super::chat::ChatExecutor;
use super::fake_proxy::{FakeProxy, STREAM_HEAD, door};
use super::test_executor::{HandClock, drain};

#[tokio::test]
async fn an_event_past_the_limit_fails_the_run_with_log_full_and_drops_the_proxy() {
    let mut proxy = FakeProxy::start().await;
    let runs = RunRegistry::new(
        Arc::new(ChatExecutor::with_event_limit(door(proxy.addr, None), 64)),
        HandClock::at(1_000).clock(),
    );
    runs.create(RunScope::Local, "r1", json!({ "model": "m" }))
        .unwrap();
    (&mut proxy.seen).await.unwrap();
    proxy.say(STREAM_HEAD);
    proxy.say("data: {\"n\":1}\n\n");
    // An event with no end in sight: 65 bytes and still going.
    proxy.say(format!("data: {}", "x".repeat(59)));

    let (frames, end) = drain(runs.events(&RunScope::Local, "r1", 0).unwrap()).await;

    assert_eq!(frames.len(), 1);
    let info = end.expect("the run ends");
    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.error.map(|e| e.code).as_deref(), Some("log_full"));
    tokio::time::timeout(Duration::from_secs(5), &mut proxy.client_left)
        .await
        .expect("the connection is dropped")
        .unwrap();
}
