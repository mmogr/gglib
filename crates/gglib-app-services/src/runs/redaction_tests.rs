//! An error's message can quote a reply, so only the run's own scope reads
//! it: this machine gets the code with fixed text for a device's run.

use std::sync::Arc;

use gglib_core::domain::runs::{RunInfo, RunStatus};
use gglib_core::ports::{RunScope, RunsPort};
use serde_json::json;

use super::RunRegistry;
use super::chat::ChatExecutor;
use super::fake_proxy::{FakeProxy, STREAM_HEAD, door};
use super::registry::OTHERS_MESSAGE;
use super::test_executor::{HandClock, drain};

const SECRET: &str = "the model said: PRIVATE-REPLY-TEXT";

/// A device's run that failed on `answer`, and the registry holding it.
async fn failed_device_run(answer: &[&str]) -> (RunRegistry, RunScope) {
    let mut proxy = FakeProxy::start().await;
    let runs = RunRegistry::new(
        Arc::new(ChatExecutor::new(door(proxy.addr, None))),
        HandClock::at(1_000).clock(),
    );
    let phone = RunScope::Device("phone".into());
    let body = json!({ "model": "m", "messages": [] });
    runs.create(phone.clone(), "p1", body).unwrap();
    (&mut proxy.seen).await.unwrap();
    for part in answer {
        proxy.say(*part);
    }
    proxy.close();
    let (_, end) = drain(runs.events(&phone, "p1", 0).unwrap()).await;
    assert_eq!(end.map(|i| i.status), Some(RunStatus::Failed));
    (runs, phone)
}

fn assert_redacted(info: &RunInfo, code: &str) {
    let error = info.error.as_ref().expect("an error");
    assert_eq!(error.code, code, "the code is kept");
    assert_eq!(error.message, OTHERS_MESSAGE);
    assert!(
        !serde_json::to_string(info)
            .unwrap()
            .contains("PRIVATE-REPLY-TEXT")
    );
}

async fn check(answer: &[&str], code: &str) {
    let (runs, phone) = failed_device_run(answer).await;
    let local = RunScope::Local;

    assert_redacted(&runs.get(&local, "p1").unwrap(), code);
    assert_redacted(&runs.list(&local).runs[0], code);
    assert_redacted(&runs.cancel(&local, "p1").unwrap(), code);

    let own = runs.get(&phone, "p1").unwrap();
    assert_eq!(own.error.map(|e| e.message).as_deref(), Some(SECRET));
    let listed = runs.list(&phone).runs.remove(0);
    assert_eq!(listed.error.map(|e| e.message).as_deref(), Some(SECRET));
    let cancelled = runs.cancel(&phone, "p1").unwrap();
    assert_eq!(cancelled.error.map(|e| e.message).as_deref(), Some(SECRET));
}

#[tokio::test]
async fn an_error_frames_text_reaches_only_the_device_that_ran_it() {
    let frame = format!(
        "data: {}\n\n",
        json!({ "error": { "message": SECRET, "code": "upstream_timeout" } })
    );
    check(&[STREAM_HEAD, &frame], "upstream_timeout").await;
}

#[tokio::test]
async fn a_refusals_text_reaches_only_the_device_that_ran_it() {
    let body =
        json!({ "error": { "message": SECRET, "type": "server_error", "code": "model_failed" } })
            .to_string();
    let head = format!(
        "HTTP/1.1 500 Internal Server Error\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    check(&[&head, &body], "model_failed").await;
}
