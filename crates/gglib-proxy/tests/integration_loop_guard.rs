//! End-to-end tests for the pre-dispatch loop/stagnation guard.
//!
//! These are the cases where the guard must not act: each drives the real
//! proxy over HTTP with a request whose replayed `messages[]` history is the
//! signal under test, and asserts an untouched round-trip for benign traffic,
//! or for any traffic with the guard off. What a trip does is
//! `integration_loop_guard_note.rs` (the first) and
//! `integration_loop_guard_end.rs` (the second).

use std::sync::Arc;

use reqwest::Client;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use gglib_core::Settings;
use gglib_core::ports::{InMemorySettings, ModelCatalogPort, ModelRuntimePort};

mod fixtures;
use fixtures::common::{
    CountingRuntime, TaggedCatalog, spawn_mock_upstream, spawn_proxy, spawn_proxy_with_runtime,
    spawn_proxy_with_settings,
};
use fixtures::loop_guard::{
    assistant_call, chat_body, dashboard_of, looping_history, repeated_read_history,
};

/// The forwarded request's own row on the dashboard: one request, and whether
/// the guard acted on it. A trip is answered 200 like a pass, so the status
/// alone cannot tell them apart.
async fn assert_forwarded_untripped(proxy_url: &str, why: &str) {
    let dashboard = dashboard_of(proxy_url).await;
    let counts = &dashboard["per_model_defects"]["test-model"];
    assert_eq!(counts["requests"].as_u64(), Some(1), "{why}: {dashboard}");
    assert_eq!(
        counts["loop_guard_trips"].as_u64(),
        Some(0),
        "{why}: {dashboard}"
    );
}
use fixtures::sse::BASIC_TEXT;

/// Reading the same file repeatedly is the ordinary shape of an agentic
/// coding turn — read, edit, re-read to verify — and must reach the model.
///
/// Before `read_file` was classified as an observation tool the guard acted
/// on the third read, and because agentic clients replay the whole
/// conversation every turn, it acted on every turn after.
#[tokio::test]
async fn repeated_file_reads_are_not_a_loop() {
    let upstream_cancel = CancellationToken::new();
    let upstream_port = spawn_mock_upstream(vec![BASIC_TEXT], upstream_cancel.clone()).await;
    let (proxy_url, proxy_cancel) = spawn_proxy(upstream_port, "test-model", vec![]).await;
    let mut body = chat_body("test-model", repeated_read_history(5));
    body["stream"] = json!(true);

    let resp = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&body)
        .send()
        .await
        .expect("proxy request");
    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await;

    assert_forwarded_untripped(&proxy_url, "five identical file reads are not a loop").await;
    proxy_cancel.cancel();
    upstream_cancel.cancel();
}

/// The instrument must actually reach the ledger over a real request. The
/// counter's whole purpose is that a person reads it and decides whether to
/// build a corrective arm; a wiring break would look exactly like a fleet
/// with nothing to report.
#[tokio::test]
async fn an_identical_result_repeat_reaches_the_dashboard() {
    let (runtime, _admit_calls) = CountingRuntime::new(1, "test-model");
    let (proxy_url, cancel) = spawn_proxy_with_runtime(runtime, "test-model", vec![]).await;

    // The agentic continuation shape: the client executes the calls, appends
    // the results, and asks the model to carry on. The history therefore ends
    // with a tool result, not with a user turn — `chat_body` appends
    // `user: "continue"`, which is a chat-shaped request and correctly clears
    // the observation, so it cannot be used here.
    let mut messages = vec![json!({ "role": "system", "content": "be helpful" })];
    messages.push(json!({ "role": "user", "content": "check the file" }));
    messages.extend(repeated_read_history(2));

    let _ = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&json!({ "model": "test-model", "stream": false, "messages": messages }))
        .send()
        .await
        .expect("proxy request");

    let status: Value = Client::new()
        .get(format!("{proxy_url}/v1/gglib/proxy/status"))
        .send()
        .await
        .expect("status request")
        .json()
        .await
        .expect("status json");

    let counts = &status["per_model_defects"]["test-model"];
    assert_eq!(
        counts["identical_result_repeats"].as_u64(),
        Some(1),
        "the repeat must reach the ledger, got {status}"
    );
    assert_eq!(
        counts["repeats_not_evaluated"].as_u64(),
        Some(0),
        "the results were joinable, so nothing went unevaluated: {status}"
    );

    cancel.cancel();
}

/// The reading ADR 0010's kill criteria rest on must reach the ledger too.
///
/// Its neighbour has had this test since the counter existed, and the reason
/// given there applies here with more force: this counter decides whether the
/// rescue that ADR 0010 introduced gets narrowed or removed. A wiring break
/// would read as a fleet where the guard never declines to act, which is the
/// answer that leaves the rescue in place.
#[tokio::test]
async fn a_rescued_repeat_reaches_the_dashboard() {
    let (runtime, _admit_calls) = CountingRuntime::new(1, "test-model");
    let (proxy_url, cancel) = spawn_proxy_with_runtime(runtime, "test-model", vec![]).await;

    // Mutating batch, three times, answers moving: only the third is a rescue.
    let mut messages = vec![json!({ "role": "user", "content": "fix the file" })];
    for (i, answer) in ["1 changed", "2 changed", "3 changed"].iter().enumerate() {
        messages.push(assistant_call("write_file", r#"{"path":"a.rs"}"#));
        messages.push(json!({
            "role": "tool", "tool_call_id": "c1", "content": format!("{answer} ({i})")
        }));
    }

    let _ = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&json!({ "model": "test-model", "stream": false, "messages": messages }))
        .send()
        .await
        .expect("proxy request");

    let status: Value = Client::new()
        .get(format!("{proxy_url}/v1/gglib/proxy/status"))
        .send()
        .await
        .expect("status request")
        .json()
        .await
        .expect("status json");

    let counts = &status["per_model_defects"]["test-model"];
    assert_eq!(
        counts["repeats_rescued"].as_u64(),
        Some(1),
        "the rescue must reach the ledger, got {status}"
    );
    assert_eq!(
        counts["identical_result_repeats"].as_u64(),
        Some(0),
        "the answers differed, so this is not an identical repeat: {status}"
    );

    cancel.cancel();
}

/// The same history with a user turn appended — the client is no longer asking
/// the model to continue the loop, so the repeat belongs to the previous
/// request and must not be counted again.
#[tokio::test]
async fn a_trailing_user_turn_does_not_re_report_the_repeat() {
    let (runtime, _admit_calls) = CountingRuntime::new(1, "test-model");
    let (proxy_url, cancel) = spawn_proxy_with_runtime(runtime, "test-model", vec![]).await;

    let _ = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&chat_body("test-model", repeated_read_history(2)))
        .send()
        .await
        .expect("proxy request");

    let status: Value = Client::new()
        .get(format!("{proxy_url}/v1/gglib/proxy/status"))
        .send()
        .await
        .expect("status request")
        .json()
        .await
        .expect("status json");

    assert_eq!(
        status["per_model_defects"]["test-model"]["identical_result_repeats"].as_u64(),
        Some(0),
        "a chat-shaped request must not re-report the previous turn: {status}"
    );

    cancel.cancel();
}

// ─── Benign traffic passes ─────────────────────────────────────────────────

/// A varied multi-turn history must round-trip untouched — the guard exists
/// to catch loops, not to tax normal agentic sessions.
#[tokio::test]
async fn benign_multi_turn_history_round_trips() {
    let upstream_cancel = CancellationToken::new();
    let upstream_port = spawn_mock_upstream(vec![BASIC_TEXT], upstream_cancel.clone()).await;
    let (proxy_url, proxy_cancel) = spawn_proxy(upstream_port, "test-model", vec![]).await;

    let history = vec![
        assistant_call("read_file", r#"{"path":"a.rs"}"#),
        json!({ "role": "tool", "tool_call_id": "c1", "content": "..." }),
        assistant_call("read_file", r#"{"path":"b.rs"}"#),
        json!({ "role": "tool", "tool_call_id": "c1", "content": "..." }),
        json!({ "role": "assistant", "content": "Both files look fine." }),
    ];
    let mut body = chat_body("test-model", history);
    body["stream"] = json!(true);

    let resp = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&body)
        .send()
        .await
        .expect("proxy request");

    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await;
    assert_forwarded_untripped(&proxy_url, "benign history must not trip the guard").await;

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}

/// Repeated observation-only tool calls (browser snapshots and the like) use
/// the elevated threshold and must pass at counts that would trip an action
/// batch.
#[tokio::test]
async fn observation_tool_repetition_passes() {
    let upstream_cancel = CancellationToken::new();
    let upstream_port = spawn_mock_upstream(vec![BASIC_TEXT], upstream_cancel.clone()).await;
    let (proxy_url, proxy_cancel) = spawn_proxy(upstream_port, "test-model", vec![]).await;

    let history: Vec<Value> = (0..4)
        .flat_map(|_| {
            vec![
                assistant_call("browser_snapshot", "{}"),
                json!({ "role": "tool", "tool_call_id": "c1", "content": "<page>" }),
            ]
        })
        .collect();
    let mut body = chat_body("test-model", history);
    body["stream"] = json!(true);

    let resp = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&body)
        .send()
        .await
        .expect("proxy request");

    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await;
    assert_forwarded_untripped(
        &proxy_url,
        "observation-only repetition within the elevated threshold must pass",
    )
    .await;

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}

// ─── The off switch ────────────────────────────────────────────────────────

/// With the guard off, even a blatantly looping history must be forwarded —
/// the escape hatch has to actually work.
#[tokio::test]
async fn disabled_guard_forwards_looping_history() {
    let upstream_cancel = CancellationToken::new();
    let upstream_port = spawn_mock_upstream(vec![BASIC_TEXT], upstream_cancel.clone()).await;

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(fixtures::common::FixedUpstream {
        port: upstream_port,
        model_name: "test-model".into(),
        slot_restore_supported: true,
        pinned: false,
    });
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(TaggedCatalog {
        name: "test-model".into(),
        tags: vec![],
        dialect: None,
    });
    let settings = InMemorySettings::with(Settings {
        loop_guard: Some(false),
        ..Settings::with_defaults()
    });
    let (proxy_url, proxy_cancel) =
        spawn_proxy_with_settings(runtime, catalog, Arc::new(settings)).await;

    let mut body = chat_body("test-model", looping_history(5));
    body["stream"] = json!(true);

    let resp = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&body)
        .send()
        .await
        .expect("proxy request");

    assert_eq!(resp.status(), 200);
    let _ = resp.bytes().await;
    assert_forwarded_untripped(&proxy_url, "a disabled guard must not act on a loop").await;

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}
