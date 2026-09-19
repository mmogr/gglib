//! End to end: a loop-guard trip reaches the dashboard under the detector that
//! raised it.
//!
//! The guard is two detectors behind one verdict, and `loop_guard_trips` used
//! to be one number over both, so nobody could ask whether *stagnation* trips
//! had become rare — the question on which ADR 0011's first kill criterion
//! decides whether the proxy keeps `StagnationDetector` in its guard (#947).
//! The verdict becomes a tally in three hops (`server.rs` reads the verdict,
//! `metrics.rs` hands it to the ledger, the ledger bumps a field), and a unit
//! test per hop proves each hop. These prove the chain: a real request that
//! trips one detector, read back off the route a dashboard reads.
//!
//! Their own file because `integration_loop_guard.rs` is frozen at its size by
//! the complexity ratchet. The request builders are shared with it through
//! `fixtures::loop_guard`, since a test binary cannot import another's private
//! functions.

use reqwest::Client;
use serde_json::{Value, json};

mod fixtures;
use fixtures::common::CountingRuntime;
use fixtures::loop_guard::{chat_body, looping_history, spawn_proxy_in_mode, stagnating_history};
use gglib_core::LoopGuardMode;

/// Send `history` to a fresh proxy, expect the guard's 400 with `code`, and
/// return what the dashboard route then says about the model.
async fn counts_after_a_trip(history: Vec<Value>, code: &str) -> Value {
    let (runtime, _admit_calls) = CountingRuntime::new(1, "test-model");
    // `refuse`, so the tally is read off the same 400 these tests were
    // written against; `note` counts the same trip on the forward's snapshot
    // instead, which `integration_loop_guard_note.rs` covers.
    let (proxy_url, cancel) =
        spawn_proxy_in_mode(runtime, "test-model", LoopGuardMode::Refuse).await;

    let resp = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&chat_body("test-model", history))
        .send()
        .await
        .expect("proxy request");
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.expect("json error body");
    assert_eq!(body["error"]["code"], code, "the wrong detector fired");

    let status: Value = Client::new()
        .get(format!("{proxy_url}/v1/proxy/status"))
        .send()
        .await
        .expect("status request")
        .json()
        .await
        .expect("status json");
    cancel.cancel();

    let snapshot = &status["recent_requests"][0]["loop_guard_trip"];
    assert!(
        snapshot.is_string(),
        "the trip is on the snapshot: {status}"
    );
    json!({ "counts": status["per_model_defects"]["test-model"], "snapshot": snapshot })
}

#[tokio::test]
async fn a_stagnation_trip_is_counted_as_stagnation() {
    let read = counts_after_a_trip(stagnating_history(6), "stagnation_detected").await;
    let counts = &read["counts"];

    assert_eq!(counts["loop_guard_stagnations"].as_u64(), Some(1), "{read}");
    assert_eq!(counts["loop_guard_loops"].as_u64(), Some(0), "{read}");
    assert_eq!(
        counts["loop_guard_trips"].as_u64(),
        Some(1),
        "the sum: {read}"
    );
    assert_eq!(counts["requests"].as_u64(), Some(1), "{read}");
    assert_eq!(read["snapshot"], "stagnation", "{read}");
}

#[tokio::test]
async fn a_loop_trip_is_counted_as_a_loop() {
    let read = counts_after_a_trip(looping_history(3), "loop_detected").await;
    let counts = &read["counts"];

    assert_eq!(counts["loop_guard_loops"].as_u64(), Some(1), "{read}");
    assert_eq!(counts["loop_guard_stagnations"].as_u64(), Some(0), "{read}");
    assert_eq!(
        counts["loop_guard_trips"].as_u64(),
        Some(1),
        "the sum: {read}"
    );
    assert_eq!(counts["requests"].as_u64(), Some(1), "{read}");
    assert_eq!(read["snapshot"], "loop", "{read}");
}
