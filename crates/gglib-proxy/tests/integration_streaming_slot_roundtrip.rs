//! Streaming + cache roundtrip integration test (issue #598).
//!
//! `integration_slot_roundtrip.rs` covers the non-streaming
//! (`cache_lifecycle::run_with_cache`) cycle; this file covers the streaming
//! path (`sse_stream::spawn_and_return` + `cache_lifecycle::resolve_cache_triple`
//! / `save_after_generation`), which is a structurally different code path —
//! restore happens synchronously before the SSE response even starts, and
//! save happens in a detached task *after* the client has finished draining
//! the response body (see `wait_for_count` below).

mod fixtures;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::StreamExt as _;
use reqwest::Client;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use fixtures::common::{
    assert_sse_canonical_envelope, parse_sse_frames, spawn_mock_upstream_with_slots_streaming,
    spawn_proxy_with_cache,
};

/// Poll `counter` until it reaches `expected` or `timeout` elapses.
///
/// Needed because of the streaming save-timing gap: inside
/// `sse_stream::spawn_and_return`'s spawned task, the channel feeding the
/// client's HTTP body is dropped (ending the client's read of the response)
/// *before* `save_after_generation` is awaited. So a client that has just
/// finished draining a streaming response cannot assume the save HTTP call
/// to the mock upstream has already landed — it must wait for it. Restore
/// has no equivalent race: `resolve_cache_triple` is awaited before the
/// response headers are ever sent.
async fn wait_for_count(counter: &AtomicU64, expected: u64, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let current = counter.load(Ordering::Relaxed);
        if current >= expected {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for counter to reach {expected}, stuck at {current}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Send a streaming chat-completion request to the proxy and drain the raw
/// SSE response bytes into a `String`.
async fn send_streaming_request(proxy_base: &str, session_id: &str, model_name: &str) -> String {
    let resp = Client::new()
        .post(format!("{proxy_base}/v1/chat/completions"))
        .header("X-Gglib-Session-Id", session_id)
        .json(&json!({
            "model": model_name,
            "messages": [{ "role": "user", "content": "hello" }],
            "stream": true
        }))
        .send()
        .await
        .expect("proxy should be running");

    assert!(
        resp.status().is_success(),
        "streaming chat completion should succeed: {}",
        resp.status()
    );
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream")
    );

    let mut body = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        body.extend_from_slice(&chunk.expect("body chunk"));
    }
    String::from_utf8(body).expect("utf-8 body")
}

const SAVE_WAIT_TIMEOUT: Duration = Duration::from_secs(2);

/// A streaming request with no prior cache must generate and then save —
/// with no restore call, since nothing is cached yet — and the SSE bytes the
/// client receives must be intact (canonical envelope, correct content,
/// `[DONE]` terminator): the cache hooks must not corrupt the frame stream.
#[tokio::test]
async fn streaming_cache_saves_after_generation_with_no_prior_cache() {
    let slot_dir = std::env::temp_dir().join(format!("gglib-stream-save-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&slot_dir);

    let upstream_cancel = CancellationToken::new();
    let (upstream_port, action_log, save_count, restore_count, last_chat_body) =
        spawn_mock_upstream_with_slots_streaming(upstream_cancel.clone(), slot_dir.clone()).await;

    let (proxy_base, proxy_cancel) =
        spawn_proxy_with_cache(upstream_port, "test-model", slot_dir.clone()).await;

    let body = send_streaming_request(&proxy_base, "stream-save-test", "test-model").await;

    let (frames, saw_done) = parse_sse_frames(&body);
    assert!(saw_done, "missing [DONE] terminator");
    assert_sse_canonical_envelope(&frames, "test-model");

    let text: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(text, "Hello, world");

    // Restore is awaited before the response even starts, so this is already
    // final: nothing was cached yet, so the existence precheck in
    // `restore_with_retry` must have skipped the network call entirely.
    assert_eq!(
        restore_count.load(Ordering::Relaxed),
        0,
        "no slot file exists yet — restore should be skipped, not attempted"
    );

    // Save happens in the background after the client's stream has already
    // ended (see `wait_for_count` doc comment) — wait for it before asserting
    // on it or on the action order.
    wait_for_count(&save_count, 1, SAVE_WAIT_TIMEOUT).await;

    let actions = action_log.lock().await.clone();
    assert_eq!(
        actions,
        vec![1, 2],
        "expected generate→save order (no restore — nothing cached yet), got: {actions:?}"
    );

    let forwarded_body = last_chat_body.lock().await.clone().expect("body captured");
    let forwarded_json: serde_json::Value = serde_json::from_slice(&forwarded_body).unwrap();
    assert_eq!(
        forwarded_json["cache_prompt"],
        serde_json::json!(true),
        "proxy must force cache_prompt=true for the streaming path too, got: {forwarded_json}"
    );

    proxy_cancel.cancel();
    upstream_cancel.cancel();
    let _ = std::fs::remove_dir_all(&slot_dir);
}

/// A repeat streaming request for a session must restore its KV cache from
/// disk before generating — but only once that session is no longer the
/// proxy's single in-RAM "hot" slot. `StreamConfig::last_loaded_session`
/// tracks only the *most recently saved* session, so a request for session
/// "a", followed by a request for a *different* session "b" (which evicts
/// "a" from the hot slot — the multi-agent workflow #595 targets), followed
/// by an identical repeat of the "a" request, must show a real restore call
/// on that third request.
#[tokio::test]
async fn streaming_cache_restores_before_generation_on_repeat_request() {
    let slot_dir =
        std::env::temp_dir().join(format!("gglib-stream-restore-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&slot_dir);

    let upstream_cancel = CancellationToken::new();
    let (upstream_port, action_log, save_count, restore_count, _last_chat_body) =
        spawn_mock_upstream_with_slots_streaming(upstream_cancel.clone(), slot_dir.clone()).await;

    let (proxy_base, proxy_cancel) =
        spawn_proxy_with_cache(upstream_port, "test-model", slot_dir.clone()).await;

    // (a) First request for session "a" — cold, nothing cached yet.
    let body_a1 = send_streaming_request(&proxy_base, "stream-a", "test-model").await;
    let (frames_a1, saw_done_a1) = parse_sse_frames(&body_a1);
    assert!(saw_done_a1, "missing [DONE] terminator on request a1");
    wait_for_count(&save_count, 1, SAVE_WAIT_TIMEOUT).await;

    // (b) Request for a *different* session "b" — also cold, and this evicts
    // "a" from the single in-RAM hot-session slot.
    let body_b = send_streaming_request(&proxy_base, "stream-b", "test-model").await;
    let (frames_b, saw_done_b) = parse_sse_frames(&body_b);
    assert!(saw_done_b, "missing [DONE] terminator on request b");
    wait_for_count(&save_count, 2, SAVE_WAIT_TIMEOUT).await;

    // (c) Identical repeat of the session "a" request. "a" is no longer hot
    // (evicted by "b" in step b), so this must hit a genuine disk restore —
    // "a"'s real slot file, written by step (a)'s save, during this same
    // proxy instance's lifetime (so the mtime staleness guard doesn't skip
    // it) — over HTTP to the mock upstream.
    let body_a2 = send_streaming_request(&proxy_base, "stream-a", "test-model").await;
    let (frames_a2, saw_done_a2) = parse_sse_frames(&body_a2);
    assert!(saw_done_a2, "missing [DONE] terminator on request a2");
    assert_sse_canonical_envelope(&frames_a2, "test-model");
    wait_for_count(&save_count, 3, SAVE_WAIT_TIMEOUT).await;

    // Restore is awaited before each response starts, so by now it already
    // reflects reality: exactly one restore, from step (c).
    assert_eq!(
        restore_count.load(Ordering::Relaxed),
        1,
        "expected exactly 1 restore call — only the repeat request should hit it"
    );

    let actions = action_log.lock().await.clone();
    assert_eq!(
        actions,
        vec![1, 2, 1, 2, 0, 1, 2],
        "expected generate→save (a), generate→save (b), restore→generate→save (a again), got: {actions:?}"
    );

    // Sanity: all three responses carried the same reconstructed content —
    // cache hooks didn't corrupt or truncate any of the SSE streams.
    for frames in [&frames_a1, &frames_b, &frames_a2] {
        let text: String = frames
            .iter()
            .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
            .collect();
        assert_eq!(text, "Hello, world");
    }

    proxy_cancel.cancel();
    upstream_cancel.cancel();
    let _ = std::fs::remove_dir_all(&slot_dir);
}
