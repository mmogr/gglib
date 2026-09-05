//! A spawned proxy plus a recording upstream, for `{model}:{profile}` tests.
//!
//! The harness records what the **upstream actually received**. That is the
//! only thing that determines how llama-server samples, so it is the only
//! thing worth pinning — hence the assertion helpers at the bottom, which read
//! the forwarded body rather than any internal state.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{Json, Router, routing::post};
use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use gglib_core::domain::{InferenceConfig, InferenceProfile};
use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};

use super::common::make_mcp_service;
use super::profile_mocks::{MODEL, NamedCatalog, ProfileSettings, RecordingRuntime};

// ─── Harness ───────────────────────────────────────────────────────────────

/// Everything a test needs to drive one request and inspect what the upstream
/// saw.
pub(crate) struct Harness {
    pub(crate) proxy_url: String,
    /// Bodies the mock upstream received, in order.
    pub(crate) forwarded: Arc<Mutex<Vec<Value>>>,
    /// Model names the runtime was asked to launch.
    pub(crate) launched: Arc<Mutex<Vec<String>>>,
    pub(crate) _cancel: CancellationToken,
}

impl Harness {
    /// POST a non-streaming chat completion and return the HTTP status.
    ///
    /// Non-streaming keeps the assertions about the *request* body free of any
    /// SSE machinery.
    pub(crate) async fn post(&self, body: Value) -> reqwest::Response {
        Client::new()
            .post(format!("{}/v1/chat/completions", self.proxy_url))
            .json(&body)
            .send()
            .await
            .expect("request reaches the proxy")
    }

    /// GET the model list as JSON.
    pub(crate) async fn models(&self) -> Value {
        Client::new()
            .get(format!("{}/v1/models", self.proxy_url))
            .send()
            .await
            .expect("request reaches the proxy")
            .json()
            .await
            .expect("model list is JSON")
    }

    /// The single body the upstream received. Panics if it saw none.
    pub(crate) fn only_forwarded(&self) -> Value {
        let bodies = self.forwarded.lock().unwrap();
        assert_eq!(bodies.len(), 1, "expected exactly one upstream call");
        bodies[0].clone()
    }
}

pub(crate) async fn spawn(
    profiles: Vec<InferenceProfile>,
    catalog_names: &[&str],
    model_defaults: Option<InferenceConfig>,
    trust_client_sampling: bool,
) -> Harness {
    spawn_with_default_profile(
        profiles,
        catalog_names,
        model_defaults,
        trust_client_sampling,
        None,
    )
    .await
}

/// As [`spawn`], with a default profile applied to bare model names — what
/// `gglib serve --profile` configures.
pub(crate) async fn spawn_with_default_profile(
    profiles: Vec<InferenceProfile>,
    catalog_names: &[&str],
    model_defaults: Option<InferenceConfig>,
    trust_client_sampling: bool,
    default_profile: Option<String>,
) -> Harness {
    let forwarded: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let cancel = CancellationToken::new();

    // Mock upstream: record the body, return a minimal non-streaming reply.
    let upstream_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let upstream_port = upstream_listener.local_addr().unwrap().port();
    let recorder = Arc::clone(&forwarded);
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |Json(body): Json<Value>| {
            let recorder = Arc::clone(&recorder);
            async move {
                recorder.lock().unwrap().push(body);
                Json(json!({
                    "id": "chatcmpl-test",
                    "object": "chat.completion",
                    "created": 0,
                    "model": MODEL,
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": "ok"},
                        "finish_reason": "stop"
                    }],
                    "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
                }))
            }
        }),
    );
    let upstream_cancel = cancel.clone();
    tokio::spawn(async move {
        axum::serve(upstream_listener, app)
            .with_graceful_shutdown(upstream_cancel.cancelled_owned())
            .await
            .ok();
    });

    // Proxy.
    let launched: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(RecordingRuntime {
        port: upstream_port,
        launched: Arc::clone(&launched),
    });
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(NamedCatalog {
        names: catalog_names.iter().map(|n| (*n).to_owned()).collect(),
        inference_defaults: model_defaults,
    });
    let mcp = make_mcp_service();
    let proxy_cancel = cancel.clone();
    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            Some(4096),
            // Device memory readable: this suite is not about the fit.
            true,
            runtime,
            catalog,
            mcp,
            proxy_cancel,
            None, // daemon_cancel: no daemon in tests
            Arc::new(ProfileSettings {
                profiles,
                trust_client_sampling,
            }),
            None, // inference_override
            default_profile,
            false,
            None,
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            std::sync::Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
            std::sync::Arc::new(gglib_core::domain::defects::ModelDefectLedger::new()),
            &gglib_core::ProxyAccessConfig::default(),
        )
        .await
        .ok();
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    Harness {
        proxy_url: format!("http://{addr}"),
        forwarded,
        launched,
        _cancel: cancel,
    }
}

/// A sparse `coding` profile: it has exactly one opinion.
pub(crate) fn coding_profile() -> InferenceProfile {
    InferenceProfile {
        name: "coding".to_owned(),
        description: None,
        config: InferenceConfig {
            temperature: Some(0.2),
            ..Default::default()
        },
        list_in_models: false,
    }
}

pub(crate) fn chat_request(model: &str) -> Value {
    json!({
        "model": model,
        "stream": false,
        "messages": [{"role": "user", "content": "hi"}],
    })
}

/// Assert a sampling parameter's value.
///
/// Compared with a tolerance because these travel as `f32` through
/// `InferenceConfig` and widen to `f64` in JSON, so an exact match on a literal
/// like `0.2` fails on the widening artifact rather than on behaviour.
#[track_caller]
pub(crate) fn assert_param(body: &Value, key: &str, expected: f64) {
    let actual = body
        .get(key)
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("{key} missing from forwarded body: {body}"));
    assert!(
        (actual - expected).abs() < 1e-6,
        "{key}: expected {expected}, got {actual}"
    );
}

/// Assert a sampling parameter never reached the wire, so llama.cpp's own
/// default applies. The normal outcome for six of the seven since ADR 0003.
#[track_caller]
pub(crate) fn assert_deferred(body: &Value, key: &str) {
    assert!(
        body.get(key).is_none(),
        "{key} must be deferred to llama.cpp, but the forwarded body carries {body}"
    );
}
