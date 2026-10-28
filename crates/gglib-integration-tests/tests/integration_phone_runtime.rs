//! gglib's proxy in front of the phone's in-process runtime, composed as a
//! phone serves it: `gglib_proxy::serve` over an `InProcessRuntime` with a
//! scripted decoder, the catalog in `gglib.db`'s schema, a bearer key, no
//! host facts and no dashboard publisher.
//!
//! The model is a real file on disk, so the fit reads the size of its
//! weights as it reads a downloaded model's, and runs on the GGUF facts its
//! row carries.

mod common;

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use common::setup_test_pool;
use gglib_core::cache_metrics::CacheMetricsStore;
use gglib_core::domain::device_budget::DeviceReading;
use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};
use gglib_core::services::CatalogPortImpl;
use gglib_core::{ApiKeySource, NewModel, ProxyAccessConfig};
use gglib_db::CoreFactory;
use gglib_engine_llama::{InProcessRuntime, Script, ScriptedDecoder};
use gglib_mcp::McpService;
use gglib_proxy::slot_eviction::DiskBudget;
use gglib_proxy::{HostFacts, ProxyObservers, ServeConfig};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// The bearer the proxy is served with, which a phone mints for each serve.
const KEY: &str = "a-key-for-this-serve";

const MIB: u64 = 1024 * 1024;

/// The phone's weights: 64 MiB.
const WEIGHTS: u64 = 64 * MIB;

/// The memory still available on the phone, under its share of 8 GiB, so
/// the budget. 90% of it less the weights leaves room for 24,672 tokens of
/// the model's cache, 2176 bytes a token at `Q8_0`: the rung below is 16,384.
const AVAILABLE: u64 = 128 * MIB;

/// A phone serving one model, `small`.
struct Phone {
    base: String,
    runtime: Arc<InProcessRuntime>,
    cancel: CancellationToken,
    served: JoinHandle<Result<(), String>>,
    _models: TempDir,
}

impl Phone {
    async fn serve() -> Self {
        let models = tempfile::tempdir().expect("a models folder");
        let weights = models.path().join("small.gguf");
        std::fs::File::create(&weights)
            .and_then(|file| file.set_len(WEIGHTS))
            .expect("the weights are written");

        let repos = CoreFactory::build_repos(setup_test_pool().await.expect("a database"));
        let mut small = NewModel::new("small".to_owned(), weights, 0.1, Utc::now());
        small.architecture = Some("llama".to_owned());
        small.context_length = Some(32_768);
        small.metadata = HashMap::from(
            [
                ("llama.block_count", "4"),
                ("llama.attention.head_count", "4"),
                ("llama.attention.head_count_kv", "4"),
                ("llama.attention.key_length", "64"),
                ("llama.attention.value_length", "64"),
            ]
            .map(|(k, v)| (k.to_owned(), v.to_owned())),
        );
        repos
            .models
            .insert(&small)
            .await
            .expect("the model is added");

        let catalog: Arc<dyn ModelCatalogPort> =
            Arc::new(CatalogPortImpl::new(Arc::clone(&repos.models)));
        let reading = DeviceReading {
            physical_bytes: 8 * 1024 * MIB,
            available_bytes: AVAILABLE,
            is_simulator: false,
        };
        let (decoder, _) = ScriptedDecoder::new(Script::new(["Hi", "."]));
        let runtime = InProcessRuntime::start(decoder, Arc::clone(&catalog), reading)
            .await
            .expect("the runtime starts");
        let runtime = Arc::new(runtime);

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let cancel = CancellationToken::new();
        let config = ServeConfig {
            listener,
            default_ctx: None,
            device_memory_readable: true,
            runtime_port: Arc::clone(&runtime) as Arc<dyn ModelRuntimePort>,
            catalog_port: catalog,
            mcp: Arc::new(McpService::new(repos.mcp_servers)),
            cancel: cancel.clone(),
            daemon_cancel: None,
            settings_repo: repos.settings,
            inference_override: None,
            default_profile: None,
            cache_enabled: false,
            slot_dir: None,
            disk_budget: DiskBudget::Auto,
            host: HostFacts::none(),
            dashboard: false,
            agent_metrics: Arc::new(CacheMetricsStore::new()),
            observers: ProxyObservers::default(),
            access: ProxyAccessConfig {
                api_key: Some(KEY.to_owned()),
                api_key_source: ApiKeySource::Flag,
                ..ProxyAccessConfig::default()
            },
        };
        let served = tokio::spawn(async move {
            gglib_proxy::serve(config)
                .await
                .map_err(|e| format!("{e:#}"))
        });
        Self {
            base,
            runtime,
            cancel,
            served,
            _models: models,
        }
    }

    /// `POST /v1/chat/completions` with `body`, carrying `key` as its bearer
    /// when there is one: the status and the body read whole.
    async fn chat(&self, key: Option<&str>, body: &Value) -> (StatusCode, String) {
        let client = Client::builder().no_proxy().build().expect("a client");
        let mut request = client
            .post(format!("{}/v1/chat/completions", self.base))
            .json(body);
        if let Some(key) = key {
            request = request.bearer_auth(key);
        }
        let response = request.send().await.expect("the proxy answers");
        let status = response.status();
        (status, response.text().await.expect("a body"))
    }

    async fn stop(self) {
        self.cancel.cancel();
        self.served
            .await
            .expect("the proxy's task ends")
            .expect("the proxy stops cleanly");
        self.runtime.shutdown().await.expect("the runtime stops");
    }
}

/// A streamed chat with `model` asking for progress, as ggchat asks.
fn streamed(model: &str) -> Value {
    json!({
        "model": model,
        "stream": true,
        "return_progress": true,
        "messages": [{"role": "user", "content": "Hello?"}],
    })
}

/// The JSON of each `data:` frame of a streamed body, `[DONE]` left out.
fn frames(body: &str) -> Vec<Value> {
    body.lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|data| data.starts_with('{'))
        .map(|data| serde_json::from_str(data).unwrap_or_else(|e| panic!("{e}: {data}")))
        .collect()
}

#[tokio::test]
async fn a_streamed_reply_reports_the_context_the_runtime_loaded() {
    let phone = Phone::serve().await;

    let (status, body) = phone.chat(Some(KEY), &streamed("small")).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.trim_end().ends_with("data: [DONE]"), "{body}");
    let frames = frames(&body);
    let answer: String = frames
        .iter()
        .filter_map(|f| {
            f.pointer("/choices/0/delta/content")
                .and_then(Value::as_str)
        })
        .collect();
    assert_eq!(answer, "Hi.");
    let target = phone
        .runtime
        .current_model()
        .await
        .expect("the model is loaded");
    assert_eq!(target.effective_ctx, 16_384, "the rung the budget fits");
    let usage = frames
        .iter()
        .find_map(|f| f.get("usage"))
        .unwrap_or_else(|| panic!("a usage frame in {body}"));
    assert_eq!(usage["context_size"], json!(target.effective_ctx));
    assert!(
        usage["prompt_tokens"].as_u64().is_some_and(|n| n > 0),
        "{usage}"
    );

    phone.stop().await;
}

#[tokio::test]
async fn a_model_the_phone_does_not_hold_is_model_not_found() {
    let phone = Phone::serve().await;

    let (status, body) = phone.chat(Some(KEY), &streamed("not-here")).await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let error: Value = serde_json::from_str(&body).expect("a JSON error");
    assert_eq!(error["error"]["code"], "model_not_found");
    assert!(phone.runtime.current_model().await.is_none());

    phone.stop().await;
}

#[tokio::test]
async fn a_chat_without_the_serves_bearer_is_401() {
    let phone = Phone::serve().await;

    let (missing, _) = phone.chat(None, &streamed("small")).await;
    let (wrong, _) = phone.chat(Some("another-key"), &streamed("small")).await;

    assert_eq!(missing, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong, StatusCode::UNAUTHORIZED);
    assert!(
        phone.runtime.current_model().await.is_none(),
        "nothing was loaded for a request refused at the door"
    );

    phone.stop().await;
}
