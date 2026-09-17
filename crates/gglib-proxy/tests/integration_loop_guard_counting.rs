//! End to end: one client request counts one loop-guard intervention.
//!
//! Under `note` the guard records no snapshot of its own — the forward does —
//! so the ledger's "one request, one count" invariant rests on the forward
//! path, which has **two** `metrics.record` sites and a retry that builds a
//! second `ForwardRequest`. Each of these drives one of those three ways to
//! be wrong.
//!
//! Their own file rather than `integration_loop_guard_note.rs`: these are
//! about the ledger, not about the note's shape, and that file is at the
//! size a new one may be.

use std::sync::Arc;

use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use gglib_core::LoopGuardMode;
use gglib_core::ports::ModelRuntimePort;

mod fixtures;
use fixtures::common::FixedUpstream;
use fixtures::loop_guard::{
    DeadThenLive, chat_body, dashboard_of, looping_history, spawn_proxy_in_mode,
    spawn_recording_upstream,
};

/// A history that trips the loop guard **and** cannot be trimmed to fit: four
/// identical batches whose results are each far larger than the whole context
/// budget, so every one of them is inside the protected tail.
fn oversized_looping_history() -> Vec<Value> {
    let huge = "x".repeat(20_000);
    (0..4)
        .flat_map(|_| {
            vec![
                json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "c1",
                        "type": "function",
                        "function": { "name": "write_file",
                                      "arguments": "{\"path\":\"src/main.rs\"}" }
                    }]
                }),
                json!({ "role": "tool", "tool_call_id": "c1", "content": huge.clone() }),
            ]
        })
        .collect()
}

/// The first of the two record sites: a noted request that then fails the
/// context budget still counts its trip.
///
/// Not a corner. A conversation long and repetitive enough to trip the guard
/// is by construction the shape that reaches the context ceiling, and under
/// `refuse` the trip was always counted because the guard recorded its own
/// snapshot before anything could abort.
#[tokio::test]
async fn a_noted_request_that_is_then_clamped_still_counts_its_trip() {
    let upstream_cancel = CancellationToken::new();
    let (upstream_port, seen) = spawn_recording_upstream(upstream_cancel.clone()).await;
    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(FixedUpstream {
        port: upstream_port,
        model_name: "test-model".into(),
        slot_restore_supported: true,
        pinned: false,
    });
    let (proxy_url, proxy_cancel) =
        spawn_proxy_in_mode(runtime, "test-model", LoopGuardMode::Note).await;

    let resp = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&chat_body("test-model", oversized_looping_history()))
        .send()
        .await
        .expect("proxy request");

    assert_eq!(
        resp.status(),
        400,
        "the conversation cannot be trimmed to fit, so it is refused on that ground"
    );
    let body: Value = resp.json().await.expect("json error body");
    assert_eq!(body["error"]["code"], "context_length_exceeded");
    assert!(
        seen.lock().unwrap().is_none(),
        "nothing reached the upstream"
    );

    let dashboard = dashboard_of(&proxy_url).await;
    let counts = &dashboard["per_model_defects"]["test-model"];
    assert_eq!(
        counts["loop_guard_trips"].as_u64(),
        Some(1),
        "the clamp abort carries the trip: {dashboard}"
    );
    assert_eq!(counts["loop_guard_loops"].as_u64(), Some(1), "{dashboard}");

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}

/// The second: a noted request whose upstream dies is retried, and the retry
/// carries the **note** but not the trip, so one client request counts one
/// intervention however many attempts it takes.
#[tokio::test]
async fn an_upstream_dead_retry_of_a_noted_request_counts_one_trip() {
    let upstream_cancel = CancellationToken::new();
    let (live, seen) = spawn_recording_upstream(upstream_cancel.clone()).await;

    // A port bound and immediately dropped: nothing is listening on it, which
    // is what an admission returning a stale port looks like.
    let dead = {
        let l = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        l.local_addr().unwrap().port()
    };

    let admits = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(DeadThenLive {
        live,
        dead,
        admits: Arc::clone(&admits),
    });
    let (proxy_url, proxy_cancel) =
        spawn_proxy_in_mode(runtime, "test-model", LoopGuardMode::Note).await;

    let mut body = chat_body("test-model", looping_history(3));
    body["stream"] = json!(true);
    let resp = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&body)
        .send()
        .await
        .expect("proxy request");
    assert_eq!(resp.status(), 200, "the retry succeeded");
    let _ = resp.bytes().await;

    assert_eq!(
        admits.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "the first admission was dead and the request was re-admitted"
    );

    // The retry carries the note — it is in `body_for_retry`.
    let forwarded: Value = serde_json::from_slice(
        &seen
            .lock()
            .unwrap()
            .clone()
            .expect("the retry reached the upstream"),
    )
    .expect("json");
    let content = forwarded["messages"]
        .as_array()
        .and_then(|m| m.last())
        .and_then(|m| m["content"].as_str())
        .expect("string content");
    assert!(content.contains("[gglib loop guard]"), "{content}");

    // It does not carry the trip.
    let dashboard = dashboard_of(&proxy_url).await;
    let counts = &dashboard["per_model_defects"]["test-model"];
    assert_eq!(
        counts["loop_guard_trips"].as_u64(),
        Some(1),
        "one request, one intervention, however many attempts: {dashboard}"
    );
    assert_eq!(counts["loop_guard_loops"].as_u64(), Some(1), "{dashboard}");

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}
