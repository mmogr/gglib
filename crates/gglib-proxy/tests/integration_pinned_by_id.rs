//! A request is the model it resolves to, by id or by name.
//!
//! A client may name a model by its catalog id (`3`) or by its name (`qwen`).
//! The proxy resolves the request once, and everything after that keys on the
//! model it found: the pin, the connection the dashboard lists, the loop
//! guard's log, the per-model counters and the `model` the stream echoes. So a
//! pinned endpoint answers to its own id, and `3` and `qwen` are one model
//! everywhere a person or the A/B eval reads them.
//!
//! The pin is enforced here by [`EnforcingPinnedRuntime`], which resolves and
//! compares ids as `gglib-runtime` does; the real guard is tested there.

mod fixtures;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::response::Response;
use axum::routing::post;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use gglib_core::LoopGuardMode;
use gglib_core::domain::loop_guard_log::LoopGuardTripEvent;
use gglib_core::ports::{LoopGuardTripSink, ModelCatalogPort, ModelRuntimePort};

use fixtures::common::{
    MockSettingsRepo, MultiModelCatalog, ResidentSimRuntime, make_mcp_service, parse_sse_frames,
};
use fixtures::loop_guard::{chat_body, dashboard_of, looping_history};
use fixtures::pinned::{EnforcingPinnedRuntime, StaticCatalog};

/// The pinned model, `qwen`, is id 3; `llama` is id 7.
fn catalog() -> StaticCatalog {
    StaticCatalog::numbered(&[(3, "qwen"), (7, "llama")])
}

/// The model names the loop guard logged a scan or a trip under.
#[derive(Default)]
struct GuardLog(Mutex<Vec<String>>);

impl LoopGuardTripSink for GuardLog {
    fn record_trip(&self, event: LoopGuardTripEvent) {
        self.0.lock().unwrap().push(event.model_name().to_owned());
    }

    fn record_scan(&self, model_name: &str, _mode: LoopGuardMode, _at_secs: u64) {
        self.0.lock().unwrap().push(model_name.to_owned());
    }
}

/// An upstream that tells the test each request has arrived, then holds it
/// until the test releases it, so the proxy's live connection can be read.
struct Held {
    port: u16,
    arrived: mpsc::UnboundedReceiver<()>,
    release: Arc<Semaphore>,
}

impl Held {
    async fn spawn(cancel: CancellationToken) -> Self {
        let (tx, arrived) = mpsc::unbounded_channel();
        let release = Arc::new(Semaphore::new(0));
        let hold = (tx, Arc::clone(&release));
        let answer = |body: &'static str, content_type: &'static str| {
            move |State((tx, release)): State<(mpsc::UnboundedSender<()>, Arc<Semaphore>)>| async move {
                tx.send(()).ok();
                release.acquire().await.expect("open").forget();
                Response::builder()
                    .header("content-type", content_type)
                    .body(Body::from(body))
                    .unwrap()
            }
        };
        let app = Router::new()
            .route(
                "/v1/chat/completions",
                post(answer(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"index\":0}]}\n\n\
                     data: [DONE]\n\n",
                    "text/event-stream",
                )),
            )
            .route(
                "/v1/embeddings",
                post(answer(
                    r#"{"object":"list","data":[{"object":"embedding","embedding":[0.5],"index":0}]}"#,
                    "application/json",
                )),
            )
            .with_state(hold);
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(cancel.cancelled_owned())
                .await
                .ok();
        });
        Self {
            port,
            arrived,
            release,
        }
    }

    /// Send `body` to `path`, and once the upstream holds it, return the model
    /// name of every connection the dashboard lists, then the answer.
    async fn exchange(
        &mut self,
        base: &str,
        path: &str,
        body: Value,
    ) -> (Vec<String>, (u16, String)) {
        let url = format!("{base}{path}");
        // The whole answer, not just its headers: a stream's headers can come
        // back before the upstream has the request.
        let mut sent = tokio::spawn(async move {
            let resp = Client::new().post(url).json(&body).send().await.unwrap();
            (
                resp.status().as_u16(),
                resp.text().await.unwrap_or_default(),
            )
        });
        tokio::select! {
            arrived = self.arrived.recv() => arrived.expect("the upstream is running"),
            answered = &mut sent => panic!("answered without the upstream: {answered:?}"),
        }
        let listed = dashboard_of(base).await["active_connections"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|c| c["model_name"].as_str().unwrap_or_default().to_owned())
            .collect();
        self.release.add_permits(1);
        (listed, sent.await.expect("the proxy answers"))
    }
}

async fn spawn_proxy(
    runtime: Arc<dyn ModelRuntimePort>,
    catalog: Arc<dyn ModelCatalogPort>,
    guard_log: Option<Arc<GuardLog>>,
) -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let observers = gglib_proxy::ProxyObservers {
        loop_guard_trips: guard_log.map(|log| log as Arc<dyn LoopGuardTripSink>),
        ..Default::default()
    };
    let token = cancel.clone();
    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            Some(4096),
            true,
            runtime,
            catalog,
            make_mcp_service(),
            token,
            None,
            Arc::new(MockSettingsRepo),
            None,
            None,
            false,
            None,
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
            observers,
            &gglib_core::ProxyAccessConfig::default(),
        )
        .await
        .ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (base, cancel)
}

/// What a chat request for `model` replaying a loop is refused with, as a 404.
async fn refusal(base: &str, model: &str) -> Value {
    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&chat_body(model, looping_history(3)))
        .send()
        .await
        .expect("the proxy answers");
    assert_eq!(resp.status(), 404, "{model}");
    resp.json::<Value>().await.expect("json")["error"].clone()
}

/// A pinned proxy answers a request for its own id as well as its name,
/// refuses another model naming both, and says `model_not_found` for an id
/// nobody has — before the loop guard scans it, so the log never sees it.
#[tokio::test]
async fn a_pinned_proxy_answers_a_request_for_its_own_id() {
    let cancel = CancellationToken::new();
    let mut upstream = Held::spawn(cancel.clone()).await;
    let runtime = EnforcingPinnedRuntime::serving(catalog(), "qwen", upstream.port);
    let guard_log = Arc::new(GuardLog::default());
    let log = Some(Arc::clone(&guard_log));
    let (base, proxy) = spawn_proxy(Arc::new(runtime), Arc::new(catalog()), log).await;

    let body = json!({ "model": "3", "messages": [{"role": "user", "content": "hi"}] });
    let (_, (status, text)) = upstream.exchange(&base, "/v1/chat/completions", body).await;
    assert_eq!(status, 200, "{text}");

    let foreign = refusal(&base, "7").await;
    assert_eq!(foreign["code"], "pinned_model_mismatch", "{foreign}");
    let message = foreign["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("'qwen'") && message.contains("'llama'"),
        "{message}"
    );
    assert_eq!(refusal(&base, "9").await["code"], "model_not_found");
    // `3`'s scan, then `7`'s scan and trip; nothing for `9`.
    assert_eq!(*guard_log.0.lock().unwrap(), ["qwen", "llama", "llama"]);
    cancel.cancel();
    proxy.cancel();
}

/// `3` and `qwen` are one model: the dashboard lists the connection, the
/// stream echoes the model, the loop guard logs its scan and its trip, and
/// the per-model counters count it, all under `qwen`.
#[tokio::test]
async fn a_request_by_id_and_by_name_is_one_model_everywhere() {
    let cancel = CancellationToken::new();
    let mut upstream = Held::spawn(cancel.clone()).await;
    let runtime = EnforcingPinnedRuntime::serving(catalog(), "qwen", upstream.port);
    let guard_log = Arc::new(GuardLog::default());
    let (base, proxy) = spawn_proxy(
        Arc::new(runtime),
        Arc::new(catalog()),
        Some(Arc::clone(&guard_log)),
    )
    .await;

    for model in ["3", "qwen"] {
        let mut body = chat_body(model, looping_history(3));
        body["stream"] = json!(true);
        let (listed, (status, text)) = upstream.exchange(&base, "/v1/chat/completions", body).await;
        assert_eq!(status, 200, "{model}: {text}");
        assert_eq!(listed, ["qwen"], "{model}: the dashboard's connection");
        let (frames, _) = parse_sse_frames(&text);
        assert!(!frames.is_empty(), "{model}: {text}");
        assert!(
            frames.iter().all(|f| f["model"] == "qwen"),
            "{model}: the echo: {text}"
        );
    }

    let logged = guard_log.0.lock().unwrap().clone();
    assert_eq!(
        logged.len(),
        4,
        "a scan and a trip for each request: {logged:?}"
    );
    assert!(logged.iter().all(|name| name == "qwen"), "{logged:?}");

    let dashboard = dashboard_of(&base).await;
    let counted = dashboard["per_model_defects"]
        .as_object()
        .expect("an object");
    assert_eq!(counted.keys().collect::<Vec<_>>(), ["qwen"], "{dashboard}");
    assert_eq!(
        counted["qwen"]["loop_guard_trips"].as_u64(),
        Some(2),
        "{dashboard}"
    );
    cancel.cancel();
    proxy.cancel();
}

/// Embeddings resolve once too: a request by id is served, and registered
/// under the model's name.
#[tokio::test]
async fn embeddings_by_id_are_served_under_the_models_name() {
    let cancel = CancellationToken::new();
    let mut upstream = Held::spawn(cancel.clone()).await;
    let catalog = Arc::new(MultiModelCatalog(vec![
        ("chat".to_owned(), vec![]),
        ("embed".to_owned(), vec!["embedding".to_owned()]),
    ]));
    let ports = [("embed".to_owned(), upstream.port)].into();
    let runtime = Arc::new(ResidentSimRuntime::over(&catalog, ports));
    let (base, proxy) = spawn_proxy(runtime, catalog, None).await;

    let body = json!({ "model": "2", "input": "hello" });
    let (listed, (status, text)) = upstream.exchange(&base, "/v1/embeddings", body).await;
    assert_eq!(status, 200, "{text}");
    assert_eq!(listed, ["embed"]);
    cancel.cancel();
    proxy.cancel();
}
