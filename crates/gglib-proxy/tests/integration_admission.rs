//! The proxy's half of the admission contract, over real HTTP.
//!
//! The scheduling rules — whose turn it is, when a swap is fair, how requests
//! are batched — live in `gglib-runtime` and are tested there against the real
//! queue. This crate cannot reach them: `gglib-proxy` depends on `gglib-core`
//! and nothing else, which is the point of the boundary.
//!
//! What is left for this file is the half that *is* the proxy's, and it is not
//! a small half:
//!
//! * The lease returned by `admit` is held for as long as the response takes —
//!   through non-streaming completion, through the streaming path's spawned
//!   task, and through an embeddings round-trip.
//! * It is released afterwards, on every exit path, so the next model swap is
//!   not blocked by a request that has already finished.
//!
//! A proxy that dropped the lease early would let a swap unload a model
//! mid-stream, and nothing else in the suite would catch it: the response would
//! still look correct, right up until the day it did not.
//! [`ResidentSimRuntime`] makes that failure loud by refusing to swap while
//! anything is in flight, so an early drop shows up as a swap that should have
//! been impossible.

mod fixtures;

use std::collections::HashMap;
use std::sync::Arc;

use reqwest::Client;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use gglib_core::ports::ModelRuntimePort;

use fixtures::common::{
    MultiModelCatalog, ResidentSimRuntime, spawn_mock_embeddings_upstream, spawn_mock_upstream,
    spawn_proxy_with_catalog,
};

const CHAT_MODEL: &str = "chat-model";
const EMBED_MODEL: &str = "embed-model";

/// One non-streaming completion frame, which the proxy re-emits as SSE.
const CHAT_STREAM: &[u8] =
    b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"index\":0}]}\n\ndata: [DONE]\n\n";

/// A proxy in front of two models: one chat, one embedding.
///
/// Returns the proxy base URL, the shared runtime (for assertions), and the
/// cancellation tokens for both the proxy and the upstreams.
async fn spawn_two_model_proxy() -> (
    String,
    Arc<ResidentSimRuntime>,
    CancellationToken,
    CancellationToken,
) {
    let upstream_cancel = CancellationToken::new();
    let chat_port = spawn_mock_upstream(vec![CHAT_STREAM], upstream_cancel.clone()).await;
    let (embed_port, _) = spawn_mock_embeddings_upstream(upstream_cancel.clone(), None).await;

    let ports = HashMap::from([
        (CHAT_MODEL.to_string(), chat_port),
        (EMBED_MODEL.to_string(), embed_port),
    ]);
    let runtime = Arc::new(ResidentSimRuntime::new(ports));
    let catalog = Arc::new(MultiModelCatalog(vec![
        (CHAT_MODEL.to_string(), vec![]),
        (EMBED_MODEL.to_string(), vec!["embedding".to_string()]),
    ]));

    let (base, proxy_cancel) =
        spawn_proxy_with_catalog(Arc::clone(&runtime) as Arc<dyn ModelRuntimePort>, catalog).await;

    (base, runtime, proxy_cancel, upstream_cancel)
}

/// Wait for the runtime to go idle, or fail loudly.
///
/// The lease on a streaming response is released by the spawned task that
/// serves it, which finishes a moment after the client has read the last byte.
/// Polling for that is the honest way to assert it; asserting immediately would
/// be a race that passes by luck.
async fn wait_until_idle(runtime: &ResidentSimRuntime) {
    for _ in 0..200 {
        if runtime.inflight() == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!(
        "the proxy never released its admission lease — {} still in flight",
        runtime.inflight()
    );
}

/// A chat request takes a lease and gives it back. If it did not, no other
/// model could ever be loaded again.
#[tokio::test]
async fn a_completed_chat_request_releases_its_lease() {
    let (base, runtime, proxy_cancel, upstream_cancel) = spawn_two_model_proxy().await;

    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&json!({
            "model": CHAT_MODEL,
            "messages": [{"role": "user", "content": "hello"}],
            "stream": true,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    // Drain the body: the lease lives as long as the stream does.
    let body = resp.text().await.unwrap();
    assert!(body.contains("[DONE]"), "stream did not complete: {body}");

    wait_until_idle(&runtime).await;
    assert_eq!(runtime.swaps(), 1, "one cold start, no more");

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}

/// The same for embeddings, which take an entirely separate path through the
/// proxy and register their own connection guard.
#[tokio::test]
async fn a_completed_embeddings_request_releases_its_lease() {
    let (base, runtime, proxy_cancel, upstream_cancel) = spawn_two_model_proxy().await;

    let resp = Client::new()
        .post(format!("{base}/v1/embeddings"))
        .json(&json!({ "model": EMBED_MODEL, "input": "hello" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let _: Value = resp.json().await.unwrap();

    wait_until_idle(&runtime).await;

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}

/// The scenario M9 exists for: a chat client and an embeddings client sharing
/// one endpoint. Both must complete, and the runtime must never have been asked
/// to swap while a request was in flight — which the simulated slot enforces by
/// refusing to.
#[tokio::test]
async fn concurrent_chat_and_embeddings_requests_both_complete() {
    let (base, runtime, proxy_cancel, upstream_cancel) = spawn_two_model_proxy().await;

    let chat = {
        let base = base.clone();
        tokio::spawn(async move {
            let resp = Client::new()
                .post(format!("{base}/v1/chat/completions"))
                .json(&json!({
                    "model": CHAT_MODEL,
                    "messages": [{"role": "user", "content": "hello"}],
                    "stream": true,
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(resp.status(), 200);
            resp.text().await.unwrap()
        })
    };

    let embed = {
        let base = base.clone();
        tokio::spawn(async move {
            let resp = Client::new()
                .post(format!("{base}/v1/embeddings"))
                .json(&json!({ "model": EMBED_MODEL, "input": "hello" }))
                .send()
                .await
                .unwrap();
            assert_eq!(resp.status(), 200);
            resp.json::<Value>().await.unwrap()
        })
    };

    let chat_body = chat.await.expect("chat task panicked");
    let embed_body = embed.await.expect("embeddings task panicked");

    assert!(chat_body.contains("[DONE]"), "chat did not finish");
    assert_eq!(
        embed_body["data"][0]["embedding"].as_array().unwrap().len(),
        3,
        "embeddings did not finish"
    );

    wait_until_idle(&runtime).await;

    // Two models, one slot: at most one swap each. The exact count depends on
    // which request reached the runtime first, so the assertion is on the
    // ceiling — anything above it means a request paid for a swap it should
    // have been batched behind.
    assert!(
        runtime.swaps() <= 2,
        "two requests cost {} swaps",
        runtime.swaps()
    );

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}

/// Two requests for the *same* model must overlap rather than serialise. The
/// lease counts references; it does not lock the model to one caller.
#[tokio::test]
async fn two_requests_for_one_model_hold_leases_concurrently() {
    let upstream_cancel = CancellationToken::new();
    // Two frames so the upstream can serve two requests without falling back to
    // the bare `[DONE]` the mock emits once its chunks are taken.
    let (embed_port, _) = spawn_mock_embeddings_upstream(upstream_cancel.clone(), None).await;

    let ports = HashMap::from([(EMBED_MODEL.to_string(), embed_port)]);
    let runtime = Arc::new(ResidentSimRuntime::new(ports));
    let catalog = Arc::new(MultiModelCatalog(vec![(
        EMBED_MODEL.to_string(),
        vec!["embedding".to_string()],
    )]));
    let (base, proxy_cancel) =
        spawn_proxy_with_catalog(Arc::clone(&runtime) as Arc<dyn ModelRuntimePort>, catalog).await;

    let requests: Vec<_> = (0..4)
        .map(|_| {
            let base = base.clone();
            tokio::spawn(async move {
                Client::new()
                    .post(format!("{base}/v1/embeddings"))
                    .json(&json!({ "model": EMBED_MODEL, "input": "hello" }))
                    .send()
                    .await
                    .unwrap()
                    .status()
                    .as_u16()
            })
        })
        .collect();

    for request in requests {
        assert_eq!(request.await.expect("request task panicked"), 200);
    }

    wait_until_idle(&runtime).await;
    assert_eq!(runtime.swaps(), 1, "one model, one load");
    assert!(
        runtime.peak_inflight() > 1,
        "four concurrent requests never overlapped — the lease is serialising them"
    );

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}

/// A request the proxy refuses before admission must not take a lease at all.
/// Reaching the runtime would mean paying for a swap to serve a request that
/// was never going to be served.
#[tokio::test]
async fn a_request_refused_before_admission_never_takes_a_lease() {
    let (base, runtime, proxy_cancel, upstream_cancel) = spawn_two_model_proxy().await;

    // An embedding-only model cannot answer a chat completion.
    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&json!({
            "model": EMBED_MODEL,
            "messages": [{"role": "user", "content": "hello"}],
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    assert_eq!(runtime.inflight(), 0);
    assert_eq!(runtime.swaps(), 0, "nothing should have been loaded");

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}

/// The dashboard has to report admission state, or the queue is invisible
/// exactly when a user most needs to see it.
#[tokio::test]
async fn proxy_status_reports_the_admission_snapshot() {
    let (base, _runtime, proxy_cancel, upstream_cancel) = spawn_two_model_proxy().await;

    let status: Value = Client::new()
        .get(format!("{base}/v1/proxy/status"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let admission = &status["admission"];
    assert!(
        admission.is_object(),
        "status must carry an admission object: {status}"
    );
    assert!(admission["slots"].is_array());
    assert!(admission["queued"].is_array());
    assert_eq!(admission["total_swaps"], json!(0));
    assert!(
        admission["secondary_slot"]["state"].is_string(),
        "the second slot must always explain itself: {admission}"
    );
    assert!(
        !admission["secondary_slot"]["detail"]
            .as_str()
            .unwrap()
            .is_empty()
    );

    proxy_cancel.cancel();
    upstream_cancel.cancel();
}
