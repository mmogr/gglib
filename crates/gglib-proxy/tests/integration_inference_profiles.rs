//! End-to-end tests for `{model}:{profile}` request routing.
//!
//! These assert on the body the **upstream actually receives**, not on
//! internal state: the mock upstream records the forwarded JSON and each test
//! inspects the sampling parameters in it. That is the only thing that
//! determines how llama-server samples, so it is the only thing worth pinning.
//!
//! The proxy is the real `gglib_proxy::serve` with mock ports, as in
//! `integration_proxy_pipeline.rs`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::{Json, Router, routing::post};
use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use gglib_core::Settings;
use gglib_core::domain::{InferenceConfig, InferenceProfile};
use gglib_core::ports::{
    CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError, ModelRuntimePort,
    ModelSummary, RepositoryError, RunningTarget, SettingsRepository,
};
use gglib_core::{McpRepositoryError, McpServer, McpServerRepository, NewMcpServer, NoopEmitter};
use gglib_mcp::McpService;

mod fixtures;
use fixtures::common::make_orchestrator_deps;

const MODEL: &str = "qwen";

// ─── Mock ports ────────────────────────────────────────────────────────────

/// Runtime that always reports the mock upstream as running, and records the
/// model name it was asked to launch.
#[derive(Debug)]
struct RecordingRuntime {
    port: u16,
    launched: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl ModelRuntimePort for RecordingRuntime {
    async fn ensure_model_running(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        self.launched.lock().unwrap().push(model_name.to_owned());
        Ok(RunningTarget::local(
            self.port,
            1,
            model_name.to_owned(),
            4096,
            false,
        ))
    }
    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

/// Catalog resolving an explicit set of names by exact match.
#[derive(Debug)]
struct NamedCatalog {
    names: Vec<String>,
    /// Per-model stored defaults, returned for every resolved model.
    inference_defaults: Option<InferenceConfig>,
}

impl NamedCatalog {
    fn summary(&self, name: &str) -> ModelSummary {
        ModelSummary {
            id: 1,
            name: name.to_owned(),
            tags: Vec::new(),
            capabilities: gglib_core::domain::ModelCapabilities::empty(),
            param_count: "7B".into(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
            context_length: None,
            inference_defaults: self.inference_defaults.clone(),
            server_defaults: None,
        }
    }
}

#[async_trait]
impl ModelCatalogPort for NamedCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(self.names.iter().map(|n| self.summary(n)).collect())
    }
    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(self
            .names
            .iter()
            .any(|n| n == name)
            .then(|| self.summary(name)))
    }
    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// Settings repository serving a fixed profile list.
struct ProfileSettings {
    profiles: Vec<InferenceProfile>,
}

#[async_trait]
impl SettingsRepository for ProfileSettings {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        Ok(Settings {
            inference_profiles: Some(self.profiles.clone()),
            ..Settings::with_defaults()
        })
    }
    async fn save(&self, _: &Settings) -> Result<(), RepositoryError> {
        Ok(())
    }
}

struct EmptyMcpRepo;

#[async_trait]
impl McpServerRepository for EmptyMcpRepo {
    async fn insert(&self, _s: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::Internal("not implemented".into()))
    }
    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(id.to_string()))
    }
    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(name.into()))
    }
    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        Ok(vec![])
    }
    async fn update(&self, _s: &McpServer) -> Result<(), McpRepositoryError> {
        Ok(())
    }
    async fn delete(&self, _id: i64) -> Result<(), McpRepositoryError> {
        Ok(())
    }
    async fn update_last_connected(&self, _id: i64) -> Result<(), McpRepositoryError> {
        Ok(())
    }
}

// ─── Harness ───────────────────────────────────────────────────────────────

/// Everything a test needs to drive one request and inspect what the upstream
/// saw.
struct Harness {
    proxy_url: String,
    /// Bodies the mock upstream received, in order.
    forwarded: Arc<Mutex<Vec<Value>>>,
    /// Model names the runtime was asked to launch.
    launched: Arc<Mutex<Vec<String>>>,
    _cancel: CancellationToken,
}

impl Harness {
    /// POST a non-streaming chat completion and return the HTTP status.
    ///
    /// Non-streaming keeps the assertions about the *request* body free of any
    /// SSE machinery.
    async fn post(&self, body: Value) -> reqwest::Response {
        Client::new()
            .post(format!("{}/v1/chat/completions", self.proxy_url))
            .json(&body)
            .send()
            .await
            .expect("request reaches the proxy")
    }

    /// GET the model list as JSON.
    async fn models(&self) -> Value {
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
    fn only_forwarded(&self) -> Value {
        let bodies = self.forwarded.lock().unwrap();
        assert_eq!(bodies.len(), 1, "expected exactly one upstream call");
        bodies[0].clone()
    }
}

async fn spawn(
    profiles: Vec<InferenceProfile>,
    catalog_names: &[&str],
    model_defaults: Option<InferenceConfig>,
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
    let mcp = Arc::new(McpService::new(
        Arc::new(EmptyMcpRepo),
        Arc::new(NoopEmitter::new()),
    ));
    let proxy_cancel = cancel.clone();
    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            4096,
            runtime,
            catalog,
            mcp,
            make_orchestrator_deps(),
            proxy_cancel,
            Arc::new(ProfileSettings { profiles }),
            None, // inference_override
            false,
            None,
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            std::sync::Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
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
fn coding_profile() -> InferenceProfile {
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

fn chat_request(model: &str) -> Value {
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
fn assert_param(body: &Value, key: &str, expected: f64) {
    let actual = body
        .get(key)
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("{key} missing from forwarded body: {body}"));
    assert!(
        (actual - expected).abs() < 1e-6,
        "{key}: expected {expected}, got {actual}"
    );
}

// ─── Tests ─────────────────────────────────────────────────────────────────

/// The core behaviour: the suffix selects the profile, and its temperature is
/// what reaches llama-server.
#[tokio::test]
async fn profile_suffix_applies_its_sampling() {
    let h = spawn(vec![coding_profile()], &[MODEL], None).await;

    let resp = h.post(chat_request("qwen:coding")).await;
    assert_eq!(resp.status(), 200);

    assert_param(&h.only_forwarded(), "temperature", 0.2);
}

/// The suffix must not reach the runtime: a profile selects sampling, never a
/// different model. If `qwen:coding` were launched as its own model it would
/// spawn a second llama-server and discard the KV cache on every switch.
///
/// The `model` field *inside* the forwarded body is deliberately left as the
/// client wrote it. llama-server ignores it (it serves whichever model is
/// loaded), and echoing it back unchanged means the client sees the id it
/// asked for rather than a silently rewritten one.
#[tokio::test]
async fn profile_suffix_is_stripped_before_the_model_is_launched() {
    let h = spawn(vec![coding_profile()], &[MODEL], None).await;

    h.post(chat_request("qwen:coding")).await;

    assert_eq!(&*h.launched.lock().unwrap(), &[MODEL.to_owned()]);
    assert_eq!(
        h.only_forwarded().get("model").and_then(Value::as_str),
        Some("qwen:coding"),
        "the client's requested id passes through untouched"
    );
}

/// A bare model name must behave exactly as before profiles existed.
#[tokio::test]
async fn bare_model_name_is_unaffected_by_a_configured_profile() {
    let h = spawn(vec![coding_profile()], &[MODEL], None).await;

    h.post(chat_request(MODEL)).await;

    // 0.7 is the hardcoded fallback — the profile's 0.2 must not leak in.
    assert_param(&h.only_forwarded(), "temperature", 0.7);
}

/// The client's own parameters sit above the profile in the hierarchy.
#[tokio::test]
async fn client_supplied_temperature_beats_the_profile() {
    let h = spawn(vec![coding_profile()], &[MODEL], None).await;

    let mut body = chat_request("qwen:coding");
    body["temperature"] = json!(1.5);
    h.post(body).await;

    assert_param(&h.only_forwarded(), "temperature", 1.5);
}

/// The invariant that makes one global profile safe across models: a
/// parameter the profile leaves unset still resolves from the model's own
/// stored defaults rather than being erased.
#[tokio::test]
async fn sparse_profile_leaves_model_defaults_intact() {
    let model_defaults = InferenceConfig {
        temperature: Some(1.0),
        top_p: Some(0.87),
        top_k: Some(20),
        ..Default::default()
    };
    let h = spawn(vec![coding_profile()], &[MODEL], Some(model_defaults)).await;

    h.post(chat_request("qwen:coding")).await;

    let body = h.only_forwarded();
    // Profile wins where it speaks; the model default survives where it is silent.
    assert_param(&body, "temperature", 0.2);
    assert_param(&body, "top_p", 0.87);
    assert_param(&body, "top_k", 20.0);
}

/// Regression for #621, end to end: a `:coding` request must not reach the
/// upstream carrying a `presence_penalty` the model tuned for its own, much
/// higher, temperature. This is the exact request shape that failed in
/// production.
#[tokio::test]
async fn sparse_profile_does_not_forward_model_penalties() {
    let model_defaults = InferenceConfig {
        temperature: Some(1.0),
        presence_penalty: Some(1.5),
        ..Default::default()
    };
    let h = spawn(vec![coding_profile()], &[MODEL], Some(model_defaults)).await;

    h.post(chat_request("qwen:coding")).await;

    let body = h.only_forwarded();
    assert_param(&body, "temperature", 0.2);
    assert_param(&body, "presence_penalty", 0.0);
}

/// A profile that was renamed or deleted must fail loudly, and must not reach
/// the upstream at all — a silently un-profiled request is the failure this
/// feature exists to prevent.
#[tokio::test]
async fn unknown_profile_suffix_is_rejected_without_calling_the_upstream() {
    let h = spawn(vec![coding_profile()], &[MODEL], None).await;

    let resp = h.post(chat_request("qwen:codeing")).await;
    assert_eq!(resp.status(), 404);

    let body: Value = resp.json().await.expect("error body is JSON");
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("profile_not_found"),
        "unexpected body: {body}"
    );
    // The message has to be actionable: it names the bad suffix and what exists.
    let message = body["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("codeing"), "unexpected message: {message}");
    assert!(message.contains("coding"), "unexpected message: {message}");

    assert!(
        h.forwarded.lock().unwrap().is_empty(),
        "rejected request must never reach the upstream"
    );
    assert!(
        h.launched.lock().unwrap().is_empty(),
        "rejected request must not launch a model"
    );
}

/// A model whose real name contains a colon keeps working, and is not
/// reinterpreted as a profile reference.
#[tokio::test]
async fn colon_bearing_model_name_still_resolves() {
    let h = spawn(vec![coding_profile()], &["qwen:27b"], None).await;

    let resp = h.post(chat_request("qwen:27b")).await;
    assert_eq!(resp.status(), 200);

    assert_eq!(&*h.launched.lock().unwrap(), &["qwen:27b".to_owned()]);
}

/// Regression guard for the dropped `max_tokens` fallback: with nothing
/// setting it, no cap may be forwarded.
#[tokio::test]
async fn no_max_tokens_is_forwarded_when_nothing_sets_one() {
    let h = spawn(vec![coding_profile()], &[MODEL], None).await;

    h.post(chat_request("qwen:coding")).await;

    let body = h.only_forwarded();
    assert!(
        body.get("max_tokens").is_none_or(Value::is_null),
        "unexpected max_tokens in forwarded body: {body}"
    );
}

/// A profile the user opted into listing shows up as its own picker entry,
/// alongside — not instead of — the bare model.
#[tokio::test]
async fn opted_in_profiles_are_listed_alongside_the_bare_model() {
    let mut listed = coding_profile();
    listed.list_in_models = true;
    let h = spawn(vec![listed], &[MODEL], None).await;

    let body = h.models().await;
    let ids: Vec<&str> = body["data"]
        .as_array()
        .expect("data array")
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();

    assert!(ids.contains(&MODEL), "bare model missing from {ids:?}");
    assert!(ids.contains(&"qwen:coding"), "variant missing from {ids:?}");
    assert_eq!(
        ids.iter().filter(|id| **id == MODEL).count(),
        1,
        "bare model must still be listed exactly once: {ids:?}"
    );
}

/// An advertised variant must actually work when a client selects it — the
/// listing and the routing have to agree.
#[tokio::test]
async fn an_advertised_variant_can_be_selected_and_used() {
    let mut listed = coding_profile();
    listed.list_in_models = true;
    let h = spawn(vec![listed], &[MODEL], None).await;

    let ids: Vec<String> = h.models().await["data"]
        .as_array()
        .expect("data array")
        .iter()
        .filter_map(|m| m["id"].as_str().map(str::to_owned))
        .collect();
    let advertised = ids
        .iter()
        .find(|id| id.contains(':') && !id.starts_with("gglib-council"))
        .expect("a variant is advertised");

    let resp = h.post(chat_request(advertised)).await;
    assert_eq!(resp.status(), 200, "advertised id {advertised} must work");
    assert_param(&h.only_forwarded(), "temperature", 0.2);
}

/// With no profile opted in, the listing is exactly what it was before this
/// feature existed.
#[tokio::test]
async fn unlisted_profiles_do_not_appear_in_the_model_list() {
    let h = spawn(vec![coding_profile()], &[MODEL], None).await;

    let body = h.models().await;
    let ids: Vec<&str> = body["data"]
        .as_array()
        .expect("data array")
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();

    assert!(
        !ids.contains(&"qwen:coding"),
        "unlisted profile leaked into {ids:?}"
    );
}
