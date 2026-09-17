//! End to end: under `note`, a tripped request reaches the model with a note.
//!
//! The half of #1052 that cannot be seen from a unit test is what the
//! *upstream* receives. These drive the real proxy over HTTP against an
//! upstream that keeps the body it was sent, so every claim about the note is
//! read off the bytes llama-server would have parsed: that the request was
//! forwarded at all, that the note is the last thing in the last message, that
//! it names what repeated, and that nothing else about the request moved.
//!
//! Their own file because `integration_loop_guard.rs` is frozen at its size by
//! the complexity ratchet, and because these are about a different answer:
//! that file's cases now ask for `refuse` explicitly.

use std::sync::Arc;

use reqwest::Client;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use gglib_core::LoopGuardMode;
use gglib_core::ports::ModelRuntimePort;

mod fixtures;
use fixtures::common::FixedUpstream;
use fixtures::loop_guard::{
    chat_body, looping_history, spawn_proxy_in_mode, spawn_recording_upstream, stagnating_history,
};

/// Send `history` to a proxy in `mode` and return what the upstream received,
/// the proxy's status code, and what the dashboard then says about the model.
async fn forward_under(mode: LoopGuardMode, history: Vec<Value>) -> (u16, Option<Value>, Value) {
    let upstream_cancel = CancellationToken::new();
    let (upstream_port, seen) = spawn_recording_upstream(upstream_cancel.clone()).await;
    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(FixedUpstream {
        port: upstream_port,
        model_name: "test-model".into(),
        slot_restore_supported: true,
        pinned: false,
    });
    let (proxy_url, proxy_cancel) = spawn_proxy_in_mode(runtime, "test-model", mode).await;

    let mut body = chat_body("test-model", history);
    body["stream"] = json!(true);
    let resp = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&body)
        .send()
        .await
        .expect("proxy request");
    let status = resp.status().as_u16();
    // Drain the stream so the forward completes before the dashboard is read.
    let _ = resp.bytes().await;

    let status_json: Value = Client::new()
        .get(format!("{proxy_url}/v1/proxy/status"))
        .send()
        .await
        .expect("status request")
        .json()
        .await
        .expect("status json");

    let forwarded = seen
        .lock()
        .unwrap()
        .clone()
        .map(|b| serde_json::from_slice(&b).expect("the forwarded body is JSON"));

    proxy_cancel.cancel();
    upstream_cancel.cancel();
    (status, forwarded, status_json)
}

/// The shape of the whole feature: a history that used to end the session with
/// a 400 now reaches the model, carrying a note that says what repeated.
#[tokio::test]
async fn a_repeated_batch_is_forwarded_with_a_note_naming_it() {
    let (status, forwarded, dashboard) =
        forward_under(LoopGuardMode::Note, looping_history(3)).await;

    assert_eq!(status, 200, "a noted request is forwarded, not refused");
    let forwarded = forwarded.expect("the upstream was reached");
    let messages = forwarded["messages"].as_array().expect("messages");

    // `chat_body` ends with a user turn, so the note lands inside it — no new
    // turn, which is what makes this delivery safe across chat templates.
    let last = messages.last().expect("a last message");
    assert_eq!(last["role"], "user");
    let content = last["content"].as_str().expect("string content");
    assert!(
        content.starts_with("continue\n\n"),
        "the person's own words come first: {content}"
    );
    assert!(
        content.contains("[gglib loop guard]"),
        "the note marks its author: {content}"
    );
    assert!(
        content.contains("write_file:"),
        "the note names the repeated batch: {content}"
    );

    // Nothing else about the request moved.
    assert_eq!(messages.len(), 8, "no turn was added: {messages:?}");
    assert_eq!(forwarded["model"], "test-model");

    let counts = &dashboard["per_model_defects"]["test-model"];
    assert_eq!(counts["loop_guard_trips"].as_u64(), Some(1), "{dashboard}");
    assert_eq!(counts["loop_guard_loops"].as_u64(), Some(1), "{dashboard}");
    assert_eq!(
        counts["loop_guard_stagnations"].as_u64(),
        Some(0),
        "{dashboard}"
    );
    assert_eq!(
        counts["requests"].as_u64(),
        Some(1),
        "one request, counted once: {dashboard}"
    );
    assert_eq!(
        dashboard["recent_requests"][0]["loop_guard_trip"], "loop",
        "the forward's own snapshot names the detector: {dashboard}"
    );
}

/// The stagnation half, and the proof that the two detectors stay separable
/// when the guard notes rather than refuses.
#[tokio::test]
async fn a_repeated_reply_is_forwarded_with_a_note_and_counted_as_stagnation() {
    let (status, forwarded, dashboard) =
        forward_under(LoopGuardMode::Note, stagnating_history(6)).await;

    assert_eq!(status, 200);
    let forwarded = forwarded.expect("the upstream was reached");
    let content = forwarded["messages"]
        .as_array()
        .and_then(|m| m.last())
        .and_then(|m| m["content"].as_str())
        .expect("string content");
    assert!(content.contains("[gglib loop guard]"), "{content}");
    assert!(
        content.contains("same response 6 times"),
        "the note names the count: {content}"
    );

    let counts = &dashboard["per_model_defects"]["test-model"];
    assert_eq!(
        counts["loop_guard_stagnations"].as_u64(),
        Some(1),
        "{dashboard}"
    );
    assert_eq!(counts["loop_guard_loops"].as_u64(), Some(0), "{dashboard}");
    assert_eq!(counts["loop_guard_trips"].as_u64(), Some(1), "{dashboard}");
    assert_eq!(
        dashboard["recent_requests"][0]["loop_guard_trip"],
        "stagnation"
    );
}

/// `off` is today's off: the same history reaches the model untouched, and
/// nothing is counted, because nothing is scanned.
#[tokio::test]
async fn an_off_guard_forwards_the_same_history_untouched() {
    let (status, forwarded, dashboard) =
        forward_under(LoopGuardMode::Off, looping_history(3)).await;

    assert_eq!(status, 200);
    let forwarded = forwarded.expect("the upstream was reached");
    let content = forwarded["messages"]
        .as_array()
        .and_then(|m| m.last())
        .and_then(|m| m["content"].as_str())
        .expect("string content");
    assert_eq!(content, "continue", "no note is added under `off`");

    let counts = &dashboard["per_model_defects"]["test-model"];
    assert_eq!(counts["loop_guard_trips"].as_u64(), Some(0), "{dashboard}");
    assert_eq!(
        counts["identical_result_repeats"].as_u64(),
        Some(0),
        "`off` scans nothing, so not even the diagnosis is taken: {dashboard}"
    );
}

/// A benign history is forwarded with nothing added, under the new default —
/// the guard taxes a loop, not a conversation.
#[tokio::test]
async fn a_benign_history_reaches_the_model_unchanged() {
    let history = vec![json!({ "role": "assistant", "content": "both files look fine" })];
    let (status, forwarded, dashboard) = forward_under(LoopGuardMode::Note, history).await;

    assert_eq!(status, 200);
    let forwarded = forwarded.expect("the upstream was reached");
    let content = forwarded["messages"]
        .as_array()
        .and_then(|m| m.last())
        .and_then(|m| m["content"].as_str())
        .expect("string content");
    assert_eq!(content, "continue");
    assert_eq!(
        dashboard["per_model_defects"]["test-model"]["loop_guard_trips"].as_u64(),
        Some(0),
        "{dashboard}"
    );
}
