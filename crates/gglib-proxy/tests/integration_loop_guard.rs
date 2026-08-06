//! End-to-end tests for the pre-dispatch loop/stagnation guard.
//!
//! Each test drives the real proxy over HTTP with a request whose replayed
//! `messages[]` history is the signal under test, and asserts the wire
//! contract an external agentic client (Cline, Roo Code) would see: a clean
//! 400 with `loop_detected` / `stagnation_detected` before any model work,
//! or an untouched 200 round-trip for benign traffic.
//!
//! The "before any model work" half of the contract is load-bearing —
//! `CountingRuntime` proves the guard fired before admission, i.e. before a
//! model swap could have been paid for a hopeless request.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use reqwest::Client;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use gglib_core::Settings;
use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};

mod fixtures;
use fixtures::common::{
    CountingRuntime, StaticSettingsRepo, TaggedCatalog, spawn_mock_upstream, spawn_proxy,
    spawn_proxy_with_runtime, spawn_proxy_with_settings,
};
use fixtures::sse::BASIC_TEXT;

// ─── Request-body builders ─────────────────────────────────────────────────

fn assistant_call(name: &str, args: &str) -> Value {
    json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [{
            "id": "c1",
            "type": "function",
            "function": { "name": name, "arguments": args }
        }]
    })
}

fn chat_body(model: &str, history: Vec<Value>) -> Value {
    let mut messages = vec![json!({ "role": "system", "content": "be helpful" })];
    messages.extend(history);
    messages.push(json!({ "role": "user", "content": "continue" }));
    json!({ "model": model, "stream": false, "messages": messages })
}

/// History with `n` identical tool-call batches (each followed by a tool
/// result, as a real client would replay it).
fn looping_history(n: usize) -> Vec<Value> {
    (0..n)
        .flat_map(|_| {
            vec![
                assistant_call("read_file", r#"{"path":"src/main.rs"}"#),
                json!({ "role": "tool", "tool_call_id": "c1", "content": "fn main() {}" }),
            ]
        })
        .collect()
}

// ─── Guard trips ───────────────────────────────────────────────────────────

/// Three identical tool-call batches → 400 `loop_detected`, with zero
/// admissions: the guard must fire before the runtime is asked to swap.
#[tokio::test]
async fn looping_history_is_rejected_before_admission() {
    let (runtime, admit_calls) = CountingRuntime::new(1, "test-model");
    let (proxy_url, cancel) = spawn_proxy_with_runtime(runtime, "test-model", vec![]).await;

    let resp = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&chat_body("test-model", looping_history(3)))
        .send()
        .await
        .expect("proxy request");

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.expect("json error body");
    assert_eq!(body["error"]["code"], "loop_detected");
    assert_eq!(body["error"]["type"], "loop_detected");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("proxy-loop-detection"),
        "error message must name the escape hatch"
    );
    assert_eq!(
        admit_calls.load(Ordering::SeqCst),
        0,
        "guard must reject before admission — no model swap for a hopeless request"
    );

    cancel.cancel();
}

/// Six identical assistant responses → 400 `stagnation_detected`.
#[tokio::test]
async fn stagnating_history_is_rejected() {
    let (runtime, admit_calls) = CountingRuntime::new(1, "test-model");
    let (proxy_url, cancel) = spawn_proxy_with_runtime(runtime, "test-model", vec![]).await;

    let history: Vec<Value> = (0..6)
        .map(|_| json!({ "role": "assistant", "content": "I cannot proceed further." }))
        .collect();
    let resp = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&chat_body("test-model", history))
        .send()
        .await
        .expect("proxy request");

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.expect("json error body");
    assert_eq!(body["error"]["code"], "stagnation_detected");
    assert_eq!(admit_calls.load(Ordering::SeqCst), 0);

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

    assert_eq!(resp.status(), 200, "benign history must not trip the guard");

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

    assert_eq!(
        resp.status(),
        200,
        "observation-only repetition within the elevated threshold must pass"
    );

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}

// ─── The off switch ────────────────────────────────────────────────────────

/// With `proxy_loop_detection = Some(false)`, even a blatantly looping
/// history must be forwarded — the escape hatch has to actually work.
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
    });
    let settings = Settings {
        proxy_loop_detection: Some(false),
        ..Settings::with_defaults()
    };
    let (proxy_url, proxy_cancel) =
        spawn_proxy_with_settings(runtime, catalog, Arc::new(StaticSettingsRepo(settings))).await;

    let mut body = chat_body("test-model", looping_history(5));
    body["stream"] = json!(true);

    let resp = Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&body)
        .send()
        .await
        .expect("proxy request");

    assert_eq!(
        resp.status(),
        200,
        "disabled guard must forward even a looping history"
    );

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}
