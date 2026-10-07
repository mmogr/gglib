//! A llama-server stand-in that wedges mid-answer, a runtime whose recycle
//! cures it, and a real `serve` between them under short stream bounds.
//!
//! The upstream answers its first request with one word and then keeps the
//! connection open in silence, the way a wedged llama-server does. Every later
//! request it never answers at all: its one slot is taken by the wedged
//! generation, so it assigns none. [`StallRuntime::stop_current`] is the
//! recycle, and after it the upstream answers normally. The runtime can hold
//! same-model requests in admission one at a time, as `SERVER_PARALLEL = 1`
//! makes the real queue do, or admit everything at once; and it can refuse a
//! recycle while `held`, as the real one does while an agent run holds the
//! model.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::Query;
use axum::{Router, body::Body, http::Response, routing::post};
use bytes::Bytes;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use gglib_core::ports::{
    Admission, AdmissionLease, AdmissionRelease, LaunchOverrides, ModelRuntimeError,
    ModelRuntimePort, RunningTarget,
};
use gglib_proxy::{ServeConfig, StreamBounds};

use super::common::TaggedCatalog;
use super::spawn::{defaults, spawn_under};

/// The model every request in these tests asks for.
pub(crate) const MODEL: &str = "stall-model";

/// Both upstream bounds at 300 ms: long enough that a busy runner does not cut
/// a healthy reply, short enough that a stall ends a test quickly. The other
/// two are bounds these tests never meet.
pub(crate) const BOUNDS: StreamBounds = StreamBounds {
    first_byte: Duration::from_millis(300),
    idle: Duration::from_millis(300),
    send: Duration::from_mins(1),
    unary: Duration::from_mins(1),
};

/// What the stand-in upstream has seen.
#[derive(Debug, Default)]
pub(crate) struct Upstream {
    /// Set by the recycle; from then on every request is answered.
    pub(crate) recycled: AtomicBool,
    /// Chat requests that reached it before the recycle.
    pub(crate) posts_while_wedged: AtomicU64,
    /// Chat requests that reached it after the recycle.
    pub(crate) posts_after_recycle: AtomicU64,
    /// KV-cache saves the proxy asked it for.
    pub(crate) saves: AtomicU64,
}

/// One SSE frame of answer text, then, if `finish`, the end of the turn.
fn sse(text: &str, finish: bool) -> String {
    let delta = serde_json::json!({"choices": [{"index": 0, "delta": {"content": text}}]});
    let mut body = format!("data: {delta}\n\n");
    if finish {
        body.push_str(
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        );
        body.push_str("data: [DONE]\n\n");
    }
    body
}

async fn chat(upstream: Arc<Upstream>) -> Response<Body> {
    let body = if upstream.recycled.load(Ordering::SeqCst) {
        upstream.posts_after_recycle.fetch_add(1, Ordering::SeqCst);
        Body::from(sse("fresh", true))
    } else if upstream.posts_while_wedged.fetch_add(1, Ordering::SeqCst) == 0 {
        let first = futures_util::stream::iter([Ok::<_, std::io::Error>(sse("Hel", false))]);
        Body::from_stream(futures_util::StreamExt::chain(
            first,
            futures_util::stream::pending(),
        ))
    } else {
        // No slot for this one: no headers, ever.
        std::future::pending::<()>().await;
        unreachable!("a pending future never resolves")
    };
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(body)
        .expect("a valid response")
}

/// A save writes the file the proxy names, as llama-server does, so the
/// proxy's rename after it finds something.
#[allow(
    clippy::unused_async,
    reason = "grandfathered at lint inheritance, #1157"
)]
async fn slot_action(
    upstream: Arc<Upstream>,
    slot_dir: PathBuf,
    action: Query<std::collections::HashMap<String, String>>,
    body: Bytes,
) -> Response<Body> {
    if action.get("action").map(String::as_str) == Some("save") {
        upstream.saves.fetch_add(1, Ordering::SeqCst);
        let named = serde_json::from_slice::<serde_json::Value>(&body).ok();
        if let Some(name) = named.as_ref().and_then(|v| v["filename"].as_str()) {
            let _ = std::fs::create_dir_all(&slot_dir);
            let _ = std::fs::write(slot_dir.join(name), b"kv");
        }
    }
    Response::new(Body::from("{}"))
}

/// Start the stand-in upstream; returns its port and what it sees.
pub(crate) async fn spawn_upstream(
    slot_dir: PathBuf,
    cancel: CancellationToken,
) -> (u16, Arc<Upstream>) {
    let upstream = Arc::new(Upstream::default());
    let (chat_state, slot_state) = (Arc::clone(&upstream), Arc::clone(&upstream));
    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post(move || chat(Arc::clone(&chat_state))),
        )
        .route(
            "/slots/0",
            post(move |action, body| {
                slot_action(Arc::clone(&slot_state), slot_dir.clone(), action, body)
            }),
        );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("bound").port();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(cancel.cancelled_owned())
            .await
            .ok();
    });
    (port, upstream)
}

/// A runtime over the stand-in: admission, with or without the one-at-a-time
/// cap, and a recycle that cures the upstream.
#[derive(Debug)]
pub(crate) struct StallRuntime {
    port: u16,
    upstream: Arc<Upstream>,
    /// `Some` holds same-model requests in admission one at a time.
    queue: Option<Arc<Queue>>,
    /// How many chat requests had reached the upstream at each recycle.
    pub(crate) recycles: std::sync::Mutex<Vec<u64>>,
    /// Whether, at each recycle, the one-at-a-time cap had a free place, so
    /// another request could have been admitted while it ran. Recorded only
    /// when there is a cap.
    pub(crate) admission_open_at_recycle: std::sync::Mutex<Vec<bool>>,
    /// While set, `recycle_current` is refused, as a run's hold refuses it.
    pub(crate) held: AtomicBool,
}

/// The one-at-a-time cap, released by the lease.
#[derive(Debug)]
struct Queue(Semaphore);

impl AdmissionRelease for Queue {
    fn release(&self, _slot: usize) {
        self.0.add_permits(1);
    }
}

impl StallRuntime {
    pub(crate) fn new(port: u16, upstream: Arc<Upstream>, one_at_a_time: bool) -> Self {
        Self {
            port,
            upstream,
            queue: one_at_a_time.then(|| Arc::new(Queue(Semaphore::new(1)))),
            recycles: std::sync::Mutex::new(Vec::new()),
            admission_open_at_recycle: std::sync::Mutex::new(Vec::new()),
            held: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl ModelRuntimePort for StallRuntime {
    async fn admit(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        let target = RunningTarget::local(self.port, 1, model_name.to_owned(), 4096, false);
        let lease = match &self.queue {
            Some(queue) => {
                queue.0.acquire().await.expect("never closed").forget();
                AdmissionLease::new(Arc::clone(queue) as Arc<dyn AdmissionRelease>, 0)
            }
            None => AdmissionLease::detached(),
        };
        Ok(Admission { target, lease })
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        let posts = &self.upstream.posts_while_wedged;
        self.recycles
            .lock()
            .expect("not poisoned")
            .push(posts.load(Ordering::SeqCst));
        if let Some(queue) = &self.queue {
            self.admission_open_at_recycle
                .lock()
                .expect("not poisoned")
                .push(queue.0.available_permits() > 0);
        }
        self.upstream.recycled.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn recycle_current(&self) -> Result<(), ModelRuntimeError> {
        if self.held.load(Ordering::SeqCst) {
            let held = "held by an agent run".to_owned();
            return Err(ModelRuntimeError::AdmissionTimeout(held));
        }
        self.stop_current().await
    }
}

/// The real `serve` under [`BOUNDS`], with the KV cache on when there is a
/// `slot_dir`; returns its base URL.
pub(crate) async fn spawn_proxy(
    runtime: Arc<StallRuntime>,
    slot_dir: Option<PathBuf>,
    cancel: CancellationToken,
) -> String {
    spawn_proxy_under(BOUNDS, runtime, vec![], slot_dir, cancel).await
}

/// [`spawn_proxy`] under `bounds`, over any runtime, its model tagged `tags`.
pub(crate) async fn spawn_proxy_under(
    bounds: StreamBounds,
    runtime: Arc<dyn ModelRuntimePort>,
    tags: Vec<String>,
    slot_dir: Option<PathBuf>,
    cancel: CancellationToken,
) -> String {
    let config = ServeConfig {
        runtime_port: runtime,
        catalog_port: Arc::new(TaggedCatalog {
            name: MODEL.into(),
            tags,
            dialect: None,
        }),
        cancel,
        cache_enabled: slot_dir.is_some(),
        slot_dir,
        ..defaults().await
    };
    spawn_under(bounds, config).await.base
}
