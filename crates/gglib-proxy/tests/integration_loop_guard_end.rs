//! End to end: under `note`, a run that trips again after its note is ended.
//!
//! The proxy notes the first trip in a run and forwards the request. When the
//! next request shows the same run tripping again, the model kept repeating
//! after it was told, and the proxy answers in its place with a finished
//! assistant message that calls no tool — the upstream is not asked (ADR
//! 0021). `refuse` and `off` are unchanged, and the last two cases pin that.
//!
//! Its own file because `integration_loop_guard_note.rs` is about the note's
//! shape and `integration_loop_guard.rs` is frozen at its size.

use std::sync::Arc;

use reqwest::Client;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use gglib_core::LoopGuardMode;
use gglib_core::ports::ModelRuntimePort;

mod fixtures;
use fixtures::common::FixedUpstream;
use fixtures::loop_guard::{
    dashboard_of, looping_history, spawn_proxy_in_mode, spawn_recording_upstream,
};

/// A proxy in `mode` in front of a recording upstream.
struct Rig {
    proxy_url: String,
    seen: Arc<std::sync::Mutex<Option<bytes::Bytes>>>,
    cancel: (CancellationToken, CancellationToken),
}

impl Rig {
    async fn new(mode: LoopGuardMode) -> Self {
        let upstream_cancel = CancellationToken::new();
        let (upstream_port, seen) = spawn_recording_upstream(upstream_cancel.clone()).await;
        let runtime: Arc<dyn ModelRuntimePort> = Arc::new(FixedUpstream {
            port: upstream_port,
            model_name: "test-model".into(),
            slot_restore_supported: true,
            pinned: false,
        });
        let (proxy_url, proxy_cancel) = spawn_proxy_in_mode(runtime, "test-model", mode).await;
        Self {
            proxy_url,
            seen,
            cancel: (proxy_cancel, upstream_cancel),
        }
    }

    /// Send `n` identical batches and return the status, the response body,
    /// and what the upstream was sent for this request, if anything.
    ///
    /// The agentic shape: the history ends with the last batch's tool result,
    /// as a client carrying on by itself sends it. A trailing user turn would
    /// be a person speaking, which the guard notes rather than ends.
    async fn send(&self, n: usize, stream: bool) -> (u16, String, Option<Value>) {
        let mut messages = vec![
            json!({ "role": "system", "content": "be helpful" }),
            json!({ "role": "user", "content": "plan the fix" }),
        ];
        messages.extend(looping_history(n));
        let body = json!({ "model": "test-model", "stream": stream, "messages": messages });
        let resp = Client::new()
            .post(format!("{}/v1/chat/completions", self.proxy_url))
            .json(&body)
            .send()
            .await
            .expect("proxy request");
        let status = resp.status().as_u16();
        let text = resp.text().await.expect("response body");
        let forwarded = self
            .seen
            .lock()
            .unwrap()
            .take()
            .map(|b| serde_json::from_slice(&b).expect("the forwarded body is JSON"));
        (status, text, forwarded)
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.cancel.0.cancel();
        self.cancel.1.cancel();
    }
}

fn last_content(forwarded: &Value) -> &str {
    forwarded["messages"]
        .as_array()
        .and_then(|m| m.last())
        .and_then(|m| m["content"].as_str())
        .expect("string content")
}

/// The whole feature: the first trip is noted and forwarded, the second is
/// answered by the proxy with a finished message, and the upstream never sees
/// the second.
#[tokio::test]
async fn a_run_that_trips_again_after_its_note_is_ended_without_the_upstream() {
    let rig = Rig::new(LoopGuardMode::Note).await;

    let (status, _, forwarded) = rig.send(3, true).await;
    assert_eq!(status, 200);
    let forwarded = forwarded.expect("the first trip is forwarded");
    assert!(
        last_content(&forwarded).contains("[gglib loop guard]"),
        "the first trip carries the note: {forwarded}"
    );

    let (status, text, forwarded) = rig.send(4, true).await;
    assert_eq!(status, 200, "the turn finished: {text}");
    assert!(forwarded.is_none(), "the upstream was not called");
    let frames: Vec<Value> = text
        .split("\n\n")
        .filter_map(|f| f.strip_prefix("data: "))
        .filter(|f| *f != "[DONE]")
        .map(|f| serde_json::from_str(f).expect("frame json"))
        .collect();
    let content: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert!(
        content.contains("stopped this run"),
        "the client is told why: {text}"
    );
    assert_eq!(
        frames
            .last()
            .map(|f| f["choices"][0]["finish_reason"].clone()),
        Some(json!("stop")),
        "{text}"
    );
    assert!(!text.contains("tool_calls"), "no tool call: {text}");
    assert!(text.ends_with("data: [DONE]\n\n"), "{text}");

    let dashboard = dashboard_of(&rig.proxy_url).await;
    let recent = &dashboard["recent_requests"];
    let newest = recent
        .as_array()
        .and_then(|r| r.last())
        .expect("two requests were recorded");
    assert_eq!(newest["loop_guard_ended"], true, "{dashboard}");
    assert_eq!(newest["loop_guard_trip"], "loop", "{dashboard}");
    assert_eq!(
        dashboard["per_model_defects"]["test-model"]["loop_guard_trips"].as_u64(),
        Some(2),
        "the note and the ending are both trips: {dashboard}"
    );
}

/// The same ending for a client that did not ask for a stream.
#[tokio::test]
async fn an_unstreamed_request_is_ended_with_a_finished_completion() {
    let rig = Rig::new(LoopGuardMode::Note).await;

    let (status, text, forwarded) = rig.send(4, false).await;
    assert_eq!(status, 200, "{text}");
    assert!(forwarded.is_none(), "the upstream was not called");
    let body: Value = serde_json::from_str(&text).expect("json body");
    let choice = &body["choices"][0];
    assert_eq!(choice["finish_reason"], "stop", "{body}");
    assert_eq!(choice["message"]["role"], "assistant", "{body}");
    assert!(choice["message"].get("tool_calls").is_none(), "{body}");
}

#[tokio::test]
async fn refuse_still_refuses_the_same_history() {
    let rig = Rig::new(LoopGuardMode::Refuse).await;
    let (status, text, forwarded) = rig.send(4, true).await;
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("loop_detected"), "{text}");
    assert!(forwarded.is_none());
}

#[tokio::test]
async fn off_still_forwards_the_same_history_untouched() {
    let rig = Rig::new(LoopGuardMode::Off).await;
    let (status, _, forwarded) = rig.send(4, true).await;
    assert_eq!(status, 200);
    let forwarded = forwarded.expect("the upstream was reached");
    assert_eq!(
        last_content(&forwarded),
        "1 file changed",
        "no note under `off`"
    );
}
