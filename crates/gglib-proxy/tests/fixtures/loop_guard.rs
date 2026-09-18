//! Request bodies the loop-guard integration tests send.
//!
//! One home for them because a test binary cannot import another's private
//! functions, so `integration_loop_guard_tally.rs` had copied three of these
//! verbatim and said so in its own module doc. A fixture is the place a second
//! caller belongs, and it leaves `integration_loop_guard.rs` — frozen at its
//! size by the complexity ratchet — the room its own new cases need.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::response::Response;
use axum::routing::post;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use gglib_core::ports::{
    Admission, LaunchOverrides, ModelCatalogPort, ModelRuntimeError, ModelRuntimePort,
    RunningTarget,
};
use gglib_core::{LoopGuardMode, Settings};

use super::common::{StaticSettingsRepo, TaggedCatalog, spawn_proxy_with_settings};

/// Spawn a proxy whose loop guard runs in `mode`.
///
/// The guard's default is `note`, so a test that wants a refusal has to ask
/// for one. Here rather than in `common.rs`, which is at its size baseline,
/// and beside the histories these tests send.
pub(crate) async fn spawn_proxy_in_mode(
    runtime: Arc<dyn ModelRuntimePort>,
    model_name: &str,
    mode: LoopGuardMode,
) -> (String, CancellationToken) {
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(TaggedCatalog {
        name: model_name.into(),
        tags: vec![],
        dialect: None,
    });
    let settings = Settings {
        loop_guard_mode: Some(mode),
        ..Settings::with_defaults()
    };
    spawn_proxy_with_settings(runtime, catalog, Arc::new(StaticSettingsRepo(settings))).await
}

/// One assistant turn carrying a single tool call.
pub(crate) fn assistant_call(name: &str, args: &str) -> Value {
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

/// A complete request body: a system turn, `history`, and a trailing user turn
/// — the shape an agentic client replays on every request.
pub(crate) fn chat_body(model: &str, history: Vec<Value>) -> Value {
    let mut messages = vec![json!({ "role": "system", "content": "be helpful" })];
    messages.extend(history);
    messages.push(json!({ "role": "user", "content": "continue" }));
    json!({ "model": model, "stream": false, "messages": messages })
}

/// History with `n` identical tool-call batches (each followed by a tool
/// result, as a real client would replay it).
///
/// Uses a *mutating* tool deliberately. `read_file` and friends are
/// observation tools, whose repeats are held to the far higher
/// `max_observation_steps` ceiling — see
/// `repeated_file_reads_are_not_a_loop` for why that matters.
pub(crate) fn looping_history(n: usize) -> Vec<Value> {
    (0..n)
        .flat_map(|_| {
            vec![
                assistant_call("write_file", r#"{"path":"src/main.rs"}"#),
                json!({ "role": "tool", "tool_call_id": "c1", "content": "1 file changed" }),
            ]
        })
        .collect()
}

/// The same shape, but with the read-only tool a coding agent repeats.
pub(crate) fn repeated_read_history(n: usize) -> Vec<Value> {
    (0..n)
        .flat_map(|_| {
            vec![
                assistant_call("read_file", r#"{"path":"src/main.rs"}"#),
                json!({ "role": "tool", "tool_call_id": "c1", "content": "fn main() {}" }),
            ]
        })
        .collect()
}

/// `n` identical assistant replies and no tool call anywhere, so the loop
/// detector never sees a batch to count.
pub(crate) fn stagnating_history(n: usize) -> Vec<Value> {
    (0..n)
        .map(|_| json!({ "role": "assistant", "content": "I cannot proceed further." }))
        .collect()
}

/// An upstream that answers every completion with one SSE frame and keeps the
/// request body it was handed.
///
/// Its own rather than `spawn_mock_upstream`'s: what these tests are about is
/// the bytes that arrive, and nothing shared captures them without also
/// dragging in the KV-cache harness.
pub(crate) async fn spawn_recording_upstream(
    cancel: CancellationToken,
) -> (u16, Arc<Mutex<Option<Bytes>>>) {
    let seen: Arc<Mutex<Option<Bytes>>> = Arc::new(Mutex::new(None));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();

    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post(
                |State(seen): State<Arc<Mutex<Option<Bytes>>>>, body: Bytes| async move {
                    *seen.lock().unwrap() = Some(body);
                    Response::builder()
                        .header("content-type", "text/event-stream")
                        .header("cache-control", "no-cache")
                        .body(Body::from(
                            "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"index\":0}]}\n\n\
                         data: [DONE]\n\n",
                        ))
                        .unwrap()
                },
            ),
        )
        .with_state(Arc::clone(&seen));

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { cancel.cancelled().await })
            .await
            .ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (port, seen)
}

/// A runtime whose first admission points at a port nothing is listening on,
/// and whose second points at `live`.
///
/// Staging `ForwardError::UpstreamDead` is the only way to reach the retry,
/// and nothing in this workspace staged it before. The proxy asks once, gets
/// a dead port, calls `stop_current`, asks again, and retries against the
/// live one — which is the path that builds a **second** `ForwardRequest` for
/// one client request.
#[derive(Debug)]
pub(crate) struct DeadThenLive {
    pub live: u16,
    pub dead: u16,
    pub admits: Arc<std::sync::atomic::AtomicU64>,
}

#[async_trait::async_trait]
impl ModelRuntimePort for DeadThenLive {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        let n = self
            .admits
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let port = if n == 0 { self.dead } else { self.live };
        Ok(Admission::detached(RunningTarget::local(
            port,
            1,
            "test-model".into(),
            4096,
            false,
        )))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    fn pinned_model(&self) -> Option<String> {
        None
    }
}

/// Read the dashboard off a running proxy.
pub(crate) async fn dashboard_of(proxy_url: &str) -> Value {
    Client::new()
        .get(format!("{proxy_url}/v1/proxy/status"))
        .send()
        .await
        .expect("status request")
        .json()
        .await
        .expect("status json")
}
