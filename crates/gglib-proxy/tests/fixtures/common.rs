//! Shared mock implementations for gglib-proxy integration tests.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::{Router, body::Body, http::Response, routing::post};
use bytes::Bytes;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use gglib_core::Settings;
use gglib_core::domain::InferenceConfig;
use gglib_core::domain::inference_profile::InferenceProfile;
use gglib_core::ports::{
    Admission, CatalogError, LaunchOverrides, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError,
    ModelRuntimePort, ModelSummary, RepositoryError, RunningTarget, SettingsRepository,
};
use gglib_core::{McpRepositoryError, McpServer, McpServerRepository, NewMcpServer};
use gglib_mcp::McpService;

// ─── ModelRuntimePort mock ────────────────────────────────────────────────

/// Runtime port that never actually launches anything.
#[derive(Debug)]
pub(crate) struct NoopRuntime;

#[async_trait]
impl ModelRuntimePort for NoopRuntime {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Ok(Admission::detached(RunningTarget::local(
            0,
            1,
            "mock".into(),
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
}

/// Runtime port that reports itself pinned to one model.
///
/// The read side of `gglib serve`: what a caller sees when the manager was
/// pinned via `ProcessManager::set_pin`. It does not enforce the pin —
/// that guard lives in `gglib-runtime` and is tested there — so a test can
/// tell the difference between "not advertised" and "refused".
#[derive(Debug)]
pub(crate) struct PinnedRuntime(pub &'static str);

#[async_trait]
impl ModelRuntimePort for PinnedRuntime {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Ok(Admission::detached(RunningTarget::local(
            0,
            1,
            self.0.into(),
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
        Some(self.0.to_string())
    }
}

/// Runtime port that *enforces* the pin, rather than only reporting it.
///
/// The write side of `gglib serve`: [`PinnedRuntime`] above deliberately
/// lets a foreign request through so catalog tests can tell "not
/// advertised" from "refused". This one refuses, so the wire contract a
/// BYOK client actually hits — 404 plus `pinned_model_mismatch` — can be
/// asserted end to end over HTTP, not just at the resident-set/error-mapping
/// unit level (`gglib-runtime`'s `manager.rs`, `gglib-proxy`'s
/// `models_tests.rs`).
#[derive(Debug)]
pub(crate) struct EnforcingPinnedRuntime(pub &'static str);

#[async_trait]
impl ModelRuntimePort for EnforcingPinnedRuntime {
    async fn admit(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        if model_name != self.0 {
            return Err(ModelRuntimeError::PinnedModelMismatch {
                expected: self.0.to_string(),
                requested: model_name.to_string(),
            });
        }
        Ok(Admission::detached(RunningTarget::local(
            0,
            1,
            self.0.into(),
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
        Some(self.0.to_string())
    }
}

// ─── ModelCatalogPort mock ────────────────────────────────────────────────

/// Catalog port with no models.
#[derive(Debug)]
pub(crate) struct EmptyCatalog;

#[async_trait]
impl ModelCatalogPort for EmptyCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(vec![])
    }

    async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(None)
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// Catalog port over a fixed set of model names.
///
/// Names are all `/v1/models` filtering cares about, so everything else is
/// filled with plausible constants rather than made configurable.
#[derive(Debug)]
pub(crate) struct StaticCatalog(pub Vec<String>);

impl StaticCatalog {
    /// Build a catalog listing the given model names.
    pub(crate) fn new(names: &[&str]) -> Self {
        Self(names.iter().map(|n| (*n).to_string()).collect())
    }

    fn summary(id: u32, name: &str) -> ModelSummary {
        ModelSummary {
            dialect: None,
            template_caps: None,
            id,
            name: name.to_string(),
            tags: vec![],
            capabilities: Default::default(),
            param_count: "7B".to_string(),
            quantization: Some("Q4_K_M".to_string()),
            architecture: Some("llama".to_string()),
            created_at: 0,
            file_size: 0,
            context_length: Some(8192),
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
        }
    }
}

#[async_trait]
impl ModelCatalogPort for StaticCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(self
            .0
            .iter()
            .enumerate()
            .map(|(i, name)| Self::summary(u32::try_from(i).unwrap_or(0) + 1, name))
            .collect())
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(self
            .0
            .iter()
            .position(|n| n == name)
            .map(|i| Self::summary(u32::try_from(i).unwrap_or(0) + 1, name)))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

// ─── SettingsRepository mock ──────────────────────────────────────────────

/// Returns default settings; save is a no-op.
pub(crate) struct MockSettingsRepo;

#[async_trait]
impl SettingsRepository for MockSettingsRepo {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        Ok(Settings::with_defaults())
    }

    async fn save(&self, _: &Settings) -> Result<(), RepositoryError> {
        Ok(())
    }
}

/// Settings repository returning a caller-supplied [`Settings`] verbatim —
/// for tests exercising settings-gated proxy behaviour (e.g. the
/// `proxy_loop_detection` off switch).
pub(crate) struct StaticSettingsRepo(pub Settings);

#[async_trait]
impl SettingsRepository for StaticSettingsRepo {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        Ok(self.0.clone())
    }

    async fn save(&self, _: &Settings) -> Result<(), RepositoryError> {
        Ok(())
    }
}

/// Settings carrying one listed inference profile, so `/v1/models` emits
/// `{model}:{name}` variant entries.
pub(crate) struct ProfileSettingsRepo(pub &'static str);

#[async_trait]
impl SettingsRepository for ProfileSettingsRepo {
    async fn load(&self) -> Result<Settings, RepositoryError> {
        Ok(Settings {
            inference_profiles: Some(vec![InferenceProfile {
                name: self.0.to_string(),
                description: None,
                config: InferenceConfig::default(),
                list_in_models: true,
            }]),
            ..Settings::with_defaults()
        })
    }

    async fn save(&self, _: &Settings) -> Result<(), RepositoryError> {
        Ok(())
    }
}

// ─── McpServerRepository mock (includes update_last_connected) ────────────

/// Empty MCP repository — list returns empty, lookups return NotFound.
pub(crate) struct EmptyMcpRepo;

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

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Build an `McpService` backed by an empty repository.
pub(crate) fn make_mcp_service() -> Arc<McpService> {
    Arc::new(McpService::new(Arc::new(EmptyMcpRepo)))
}

// ─── Mock upstream (chat-completions + optional slots) ────────────────────
//
// Shared by the SSE pipeline tests (`integration_proxy_pipeline.rs`), the
// disk-cache roundtrip tests (`integration_slot_roundtrip.rs`), and the
// streaming+cache roundtrip tests (`integration_streaming_slot_roundtrip.rs`)
// — consolidated here so all three stop hand-rolling their own copies of the
// same `ModelRuntimePort`/`ModelCatalogPort` mocks and mock-upstream servers.

/// Runtime port that hands back a fixed upstream port.
///
/// `slot_restore_supported` mirrors `RunningTarget::slot_restore_supported` —
/// false models a sliding-window/hybrid/recurrent model, where the proxy must
/// bypass the disk slot layer entirely. `pinned` only affects
/// [`ModelRuntimePort::pinned_model`] — enforcement lives in `gglib-runtime`'s
/// the resident set and is out of scope here.
#[derive(Debug)]
pub(crate) struct FixedUpstream {
    pub port: u16,
    pub model_name: String,
    pub slot_restore_supported: bool,
    pub pinned: bool,
}

#[async_trait]
impl ModelRuntimePort for FixedUpstream {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Ok(Admission::detached(
            RunningTarget::local(self.port, 1, self.model_name.clone(), 4096, false)
                .with_slot_restore_supported(self.slot_restore_supported),
        ))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    fn pinned_model(&self) -> Option<String> {
        self.pinned.then(|| self.model_name.clone())
    }
}

/// Runtime port that counts how many times a launch was requested.
///
/// The pre-admission guards are only worth anything if they run *before*
/// `admit`. A 4xx alone cannot show that — it would look the same if the proxy
/// had already unloaded a model to discover the request was hopeless. This
/// makes the distinction assertable.
#[derive(Debug)]
pub(crate) struct CountingRuntime {
    pub port: u16,
    pub model_name: String,
    pub admit_calls: Arc<AtomicU64>,
}

impl CountingRuntime {
    pub(crate) fn new(port: u16, model_name: &str) -> (Arc<Self>, Arc<AtomicU64>) {
        let admit_calls = Arc::new(AtomicU64::new(0));
        let runtime = Arc::new(Self {
            port,
            model_name: model_name.into(),
            admit_calls: admit_calls.clone(),
        });
        (runtime, admit_calls)
    }
}

#[async_trait]
impl ModelRuntimePort for CountingRuntime {
    async fn admit(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        self.admit_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Admission::detached(RunningTarget::local(
            self.port,
            1,
            self.model_name.clone(),
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
}

/// Catalog port that always resolves the requested model with the given tags
/// and (optionally) a persisted dialect spec.
#[derive(Debug)]
pub(crate) struct TaggedCatalog {
    pub name: String,
    pub tags: Vec<String>,
    pub dialect: Option<gglib_core::domain::DialectSpec>,
}

impl TaggedCatalog {
    fn summary(&self) -> ModelSummary {
        ModelSummary {
            dialect: self.dialect.clone(),
            template_caps: None,
            id: 1,
            name: self.name.clone(),
            tags: self.tags.clone(),
            capabilities: gglib_core::domain::ModelCapabilities::empty(),
            param_count: "7B".into(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
            context_length: None,
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
        }
    }
}

#[async_trait]
impl ModelCatalogPort for TaggedCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(vec![self.summary()])
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        if name == self.name {
            Ok(Some(self.summary()))
        } else {
            Ok(None)
        }
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// Catalog holding several models, each with its own tags.
///
/// [`TaggedCatalog`] can only ever describe one model, which is enough for
/// tests about a single request but useless for anything involving a swap —
/// there has to be something to swap *to*.
#[derive(Debug)]
pub(crate) struct MultiModelCatalog(pub Vec<(String, Vec<String>)>);

impl MultiModelCatalog {
    fn summary_for(&self, name: &str) -> Option<ModelSummary> {
        self.0
            .iter()
            .position(|(n, _)| n == name)
            .map(|index| self.summary_at(index))
    }

    fn summary_at(&self, index: usize) -> ModelSummary {
        let (name, tags) = &self.0[index];
        ModelSummary {
            dialect: None,
            template_caps: None,
            id: u32::try_from(index).unwrap_or(0) + 1,
            name: name.clone(),
            tags: tags.clone(),
            capabilities: gglib_core::domain::ModelCapabilities::empty(),
            param_count: "7B".into(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
            context_length: None,
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
        }
    }
}

#[async_trait]
impl ModelCatalogPort for MultiModelCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok((0..self.0.len()).map(|i| self.summary_at(i)).collect())
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(self.summary_for(name))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// A runtime that simulates one VRAM slot, leases and all.
///
/// The real queue lives in `gglib-runtime`, which this crate cannot depend on
/// (and is tested there). What this double exists to pin is the *proxy's* half
/// of the contract: that it holds the lease for as long as the response takes —
/// including across the streaming path's spawned task — and releases it on
/// every exit. A proxy that dropped the lease early would let a swap cut off a
/// live stream, and nothing else in the test suite would notice.
///
/// The simulation is deliberately strict: a swap while anything is in flight is
/// a panic, not a silent reordering, so a leaked or early-dropped lease fails
/// loudly at the point of the bug.
#[derive(Debug)]
pub(crate) struct ResidentSimRuntime {
    /// Port each model's upstream listens on.
    pub ports: HashMap<String, u16>,
    slot: Arc<ResidentSimSlot>,
}

/// The simulated slot, shared with every lease it issues.
///
/// Separate from the runtime because `AdmissionRelease` is what a lease holds,
/// and `ModelRuntimePort::admit` takes `&self` rather than `&Arc<Self>` — so
/// the runtime cannot hand out an `Arc` of itself.
#[derive(Debug, Default)]
pub(crate) struct ResidentSimSlot(StdMutex<ResidentSimState>);

#[derive(Debug, Default)]
struct ResidentSimState {
    loaded: Option<String>,
    inflight: u32,
    swaps: u64,
    /// Highest concurrent in-flight count observed, so a test can prove two
    /// requests really did overlap rather than happening to serialise.
    peak_inflight: u32,
}

impl gglib_core::ports::AdmissionRelease for ResidentSimSlot {
    fn release(&self, _slot: usize) {
        let mut state = self.0.lock().unwrap();
        state.inflight = state.inflight.saturating_sub(1);
    }
}

impl ResidentSimRuntime {
    #[must_use]
    pub(crate) fn new(ports: HashMap<String, u16>) -> Self {
        Self {
            ports,
            slot: Arc::new(ResidentSimSlot::default()),
        }
    }

    /// How many model swaps this runtime has performed.
    pub(crate) fn swaps(&self) -> u64 {
        self.slot.0.lock().unwrap().swaps
    }

    /// How many requests are currently holding a lease.
    pub(crate) fn inflight(&self) -> u32 {
        self.slot.0.lock().unwrap().inflight
    }

    /// The most requests that were ever in flight at once.
    pub(crate) fn peak_inflight(&self) -> u32 {
        self.slot.0.lock().unwrap().peak_inflight
    }
}

#[async_trait]
impl ModelRuntimePort for ResidentSimRuntime {
    async fn admit(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        let port = *self
            .ports
            .get(model_name)
            .ok_or_else(|| ModelRuntimeError::ModelNotFound(model_name.to_string()))?;

        // Wait for the slot to go idle before swapping, exactly as the real
        // queue does. Polling rather than notifying keeps the double small; the
        // real scheduling is tested in `gglib-runtime`.
        loop {
            {
                let mut state = self.slot.0.lock().unwrap();
                if state.loaded.as_deref() == Some(model_name) {
                    state.inflight += 1;
                    state.peak_inflight = state.peak_inflight.max(state.inflight);
                    break;
                }
                if state.inflight == 0 {
                    state.loaded = Some(model_name.to_string());
                    state.swaps += 1;
                    state.inflight = 1;
                    state.peak_inflight = state.peak_inflight.max(1);
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        Ok(Admission {
            target: RunningTarget::local(port, 1, model_name.to_string(), 4096, false),
            lease: gglib_core::ports::AdmissionLease::new(
                Arc::clone(&self.slot) as Arc<dyn gglib_core::ports::AdmissionRelease>,
                0,
            ),
        })
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

/// Spawn a mock upstream HTTP server that yields `chunks` (in order) when
/// `POST /v1/chat/completions` is called. Returns the bound port.
///
/// Each chunk is sent as a separate body frame so tests can deliberately
/// split SSE frames across byte boundaries.
pub(crate) async fn spawn_mock_upstream(
    chunks: Vec<&'static [u8]>,
    cancel: CancellationToken,
) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();

    // Wrap chunks in a Mutex<Option<...>> so the handler can take them once
    // (axum requires Fn handlers; we serve a single request per upstream).
    let slot: Arc<StdMutex<Option<Vec<&'static [u8]>>>> = Arc::new(StdMutex::new(Some(chunks)));

    let app = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let slot = slot.clone();
            async move {
                let chunks = slot
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap_or_else(|| vec![b"data: [DONE]\n\n" as &[u8]]);
                let stream = futures_util::stream::iter(
                    chunks
                        .into_iter()
                        .map(|c| Ok::<Bytes, std::io::Error>(Bytes::from_static(c))),
                );
                Response::builder()
                    .header("content-type", "text/event-stream")
                    .header("cache-control", "no-cache")
                    .body(Body::from_stream(stream))
                    .unwrap()
            }
        }),
    );

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(cancel.cancelled_owned())
            .await
            .ok();
    });

    // Give the listener a moment to start accepting.
    tokio::time::sleep(Duration::from_millis(30)).await;
    port
}

/// Spawn a mock upstream serving `POST /v1/embeddings` with a canned
/// OpenAI-shaped response, optionally failing with `status` instead.
///
/// Returns `(port, last_body)`, where `last_body` captures the raw bytes of
/// the most recent request — the proxy is a pass-through for this endpoint, so
/// asserting on what actually arrived upstream is the only way to prove the
/// client's `input` was not reshaped on the way.
pub(crate) async fn spawn_mock_embeddings_upstream(
    cancel: CancellationToken,
    failure: Option<(u16, &'static str)>,
) -> (u16, Arc<Mutex<Option<Bytes>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();
    let last_body: Arc<Mutex<Option<Bytes>>> = Arc::new(Mutex::new(None));

    let captured = last_body.clone();
    let app = Router::new().route(
        "/v1/embeddings",
        post(move |body: Bytes| {
            let captured = captured.clone();
            async move {
                *captured.lock().await = Some(body);
                if let Some((status, message)) = failure {
                    return Response::builder()
                        .status(status)
                        .header("content-type", "application/json")
                        .body(Body::from(
                            json!({ "error": { "message": message } }).to_string(),
                        ))
                        .unwrap();
                }
                Response::builder()
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "object": "list",
                            "model": "embed-model",
                            "data": [{
                                "object": "embedding",
                                "index": 0,
                                "embedding": [0.1, 0.2, 0.3],
                            }],
                            "usage": { "prompt_tokens": 4, "total_tokens": 4 },
                        })
                        .to_string(),
                    ))
                    .unwrap()
            }
        }),
    );

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(cancel.cancelled_owned())
            .await
            .ok();
    });

    tokio::time::sleep(Duration::from_millis(30)).await;
    (port, last_body)
}

/// Spawn a mock upstream server that records action order for the disk-slot
/// save/restore lifecycle, and serves a plain **non-streaming** JSON response
/// from `/v1/chat/completions`.
///
/// Returns `(port, action_log, save_count, restore_count, last_chat_body)`
/// where `action_log` is a mutex-protected byte vector: `0` = restore,
/// `1` = generate, `2` = save; `last_chat_body` captures the raw bytes of
/// the most recent `/v1/chat/completions` request, for asserting on what
/// the proxy actually forwarded upstream (e.g. injected fields).
///
/// On a save action, this actually writes the requested `filename` (gglib
/// sends a per-attempt temp name, see `slots::save_slot`) under `slot_dir` —
/// real llama-server does the equivalent, writing into its
/// `--slot-save-path`. Without this, gglib's post-save `rename(tmp, final)`
/// would always fail (nothing was ever written), turning every "successful"
/// save into a `Transient` failure and retry storm.
pub(crate) async fn spawn_mock_upstream_with_slots(
    cancel: CancellationToken,
    slot_dir: PathBuf,
) -> (
    u16,
    Arc<Mutex<Vec<u8>>>,
    Arc<AtomicU64>,
    Arc<AtomicU64>,
    Arc<Mutex<Option<Bytes>>>,
) {
    spawn_mock_upstream_with_slots_impl(cancel, slot_dir, ChatResponseMode::Json).await
}

/// Same as [`spawn_mock_upstream_with_slots`], but `/v1/chat/completions`
/// responds with a **streaming** `text/event-stream` body (the fixed
/// [`super::sse::BASIC_TEXT`] fixture) instead of a single JSON object — for
/// exercising the streaming+cache code path (`sse_stream::spawn_and_return`)
/// rather than the non-streaming one (`cache_lifecycle::run_with_cache`).
pub(crate) async fn spawn_mock_upstream_with_slots_streaming(
    cancel: CancellationToken,
    slot_dir: PathBuf,
) -> (
    u16,
    Arc<Mutex<Vec<u8>>>,
    Arc<AtomicU64>,
    Arc<AtomicU64>,
    Arc<Mutex<Option<Bytes>>>,
) {
    spawn_mock_upstream_with_slots_impl(cancel, slot_dir, ChatResponseMode::Sse).await
}

/// How the mock upstream's `/v1/chat/completions` handler responds — shared
/// by the JSON (non-streaming) and SSE (streaming) slot-roundtrip mocks so
/// the `/slots/0` save/restore handler (identical in both) isn't duplicated.
enum ChatResponseMode {
    Json,
    Sse,
}

async fn spawn_mock_upstream_with_slots_impl(
    cancel: CancellationToken,
    slot_dir: PathBuf,
    mode: ChatResponseMode,
) -> (
    u16,
    Arc<Mutex<Vec<u8>>>,
    Arc<AtomicU64>,
    Arc<AtomicU64>,
    Arc<Mutex<Option<Bytes>>>,
) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock upstream");
    let port = listener.local_addr().unwrap().port();

    let action_log: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let save_count = Arc::new(AtomicU64::new(0));
    let restore_count = Arc::new(AtomicU64::new(0));
    let last_chat_body: Arc<Mutex<Option<Bytes>>> = Arc::new(Mutex::new(None));

    let log_c = action_log.clone();
    let save_n = save_count.clone();
    let restore_n = restore_count.clone();
    let last_body_c = last_chat_body.clone();

    let chat_route = {
        let log_c = log_c.clone();
        let last_body_c = last_body_c.clone();
        match mode {
            ChatResponseMode::Json => post(move |body: Bytes| {
                let log_c = log_c.clone();
                let last_body_c = last_body_c.clone();
                async move {
                    log_c.lock().await.push(1);
                    *last_body_c.lock().await = Some(body);
                    let body = json!({
                        "id": "test-123",
                        "object": "chat.completion",
                        "model": "test-model",
                        "choices": [{
                            "index": 0,
                            "message": { "role": "assistant", "content": "hello" },
                            "finish_reason": "stop"
                        }]
                    })
                    .to_string();
                    Response::builder()
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap()
                }
            }),
            ChatResponseMode::Sse => post(move |body: Bytes| {
                let log_c = log_c.clone();
                let last_body_c = last_body_c.clone();
                async move {
                    log_c.lock().await.push(1);
                    *last_body_c.lock().await = Some(body);
                    let stream = futures_util::stream::iter(vec![Ok::<Bytes, std::io::Error>(
                        Bytes::from_static(super::sse::BASIC_TEXT),
                    )]);
                    Response::builder()
                        .header("content-type", "text/event-stream")
                        .header("cache-control", "no-cache")
                        .body(Body::from_stream(stream))
                        .unwrap()
                }
            }),
        }
    };

    let app = Router::new()
        .route("/v1/chat/completions", chat_route)
        // Slot save/restore handler — records action `0` (restore) or `2` (save)
        .route(
            "/slots/0",
            post(
                move |params: axum::extract::Query<HashMap<String, String>>, body: Bytes| {
                    let log = log_c.clone();
                    let save_n = save_n.clone();
                    let restore_n = restore_n.clone();
                    let slot_dir = slot_dir.clone();
                    async move {
                        if let Some(action) = params.get("action") {
                            match action.as_str() {
                                "restore" => {
                                    log.lock().await.push(0);
                                    restore_n.fetch_add(1, Ordering::Relaxed);
                                }
                                "save" => {
                                    // Mirror real llama-server: write the requested
                                    // filename under the slot-save path so gglib's
                                    // post-save rename(tmp, final) has something to
                                    // find.
                                    if let Ok(payload) =
                                        serde_json::from_slice::<serde_json::Value>(&body)
                                        && let Some(filename) =
                                            payload.get("filename").and_then(|v| v.as_str())
                                    {
                                        let _ = std::fs::create_dir_all(&slot_dir);
                                        let _ = std::fs::write(
                                            slot_dir.join(filename),
                                            b"fake kv state",
                                        );
                                    }
                                    log.lock().await.push(2);
                                    save_n.fetch_add(1, Ordering::Relaxed);
                                }
                                _ => {}
                            }
                        }
                        Response::builder()
                            .status(200)
                            .body(Body::from("{}"))
                            .unwrap()
                    }
                },
            ),
        );

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(cancel.cancelled_owned())
            .await
            .ok();
    });

    // Give the mock server time to start listening.
    tokio::time::sleep(Duration::from_millis(30)).await;
    (port, action_log, save_count, restore_count, last_chat_body)
}

/// Spawn a proxy server with cache disabled, pointing at the given upstream
/// port. Returns `(proxy_base_url, cancel)`.
pub(crate) async fn spawn_proxy(
    upstream_port: u16,
    model_name: &str,
    tags: Vec<String>,
) -> (String, CancellationToken) {
    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(FixedUpstream {
        port: upstream_port,
        model_name: model_name.into(),
        slot_restore_supported: true,
        pinned: false,
    });
    spawn_proxy_with_runtime(runtime, model_name, tags).await
}

/// [`spawn_proxy`] with the runtime port supplied by the caller — for tests
/// that need to observe what the proxy asked the runtime to do (see
/// [`CountingRuntime`]).
pub(crate) async fn spawn_proxy_with_runtime(
    runtime: Arc<dyn ModelRuntimePort>,
    model_name: &str,
    tags: Vec<String>,
) -> (String, CancellationToken) {
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(TaggedCatalog {
        name: model_name.into(),
        tags,
        dialect: None,
    });
    spawn_proxy_with_catalog(runtime, catalog).await
}

/// [`spawn_proxy`] whose catalog row carries a persisted dialect spec —
/// the template-derived path, where no `format:*` tag is involved.
pub(crate) async fn spawn_proxy_with_dialect(
    upstream_port: u16,
    model_name: &str,
    dialect: gglib_core::domain::DialectSpec,
) -> (String, CancellationToken) {
    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(FixedUpstream {
        port: upstream_port,
        model_name: model_name.into(),
        slot_restore_supported: true,
        pinned: false,
    });
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(TaggedCatalog {
        name: model_name.into(),
        tags: Vec::new(),
        dialect: Some(dialect),
    });
    spawn_proxy_with_catalog(runtime, catalog).await
}

/// [`spawn_proxy_with_runtime`] with the catalog supplied too — for tests that
/// need more than one model to exist, which is the only way to exercise a swap.
pub(crate) async fn spawn_proxy_with_catalog(
    runtime: Arc<dyn ModelRuntimePort>,
    catalog: Arc<dyn ModelCatalogPort>,
) -> (String, CancellationToken) {
    spawn_proxy_with_settings(runtime, catalog, Arc::new(MockSettingsRepo)).await
}

/// [`spawn_proxy_with_catalog`] with the settings repository supplied too —
/// for tests exercising settings-gated behaviour (see [`StaticSettingsRepo`]).
pub(crate) async fn spawn_proxy_with_settings(
    runtime: Arc<dyn ModelRuntimePort>,
    catalog: Arc<dyn ModelCatalogPort>,
    settings_repo: Arc<dyn SettingsRepository>,
) -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let mcp = make_mcp_service();

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            Some(4096),
            runtime,
            catalog,
            mcp,
            cancel_clone,
            settings_repo,
            None, // inference_override
            None, // default_profile
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

    tokio::time::sleep(Duration::from_millis(30)).await;
    (format!("http://{addr}"), cancel)
}

/// Spawn a proxy server with cache enabled, pointing at the given upstream
/// port, with control over whether the upstream model supports disk slot
/// restore, and whether the runtime reports itself pinned.
pub(crate) async fn spawn_proxy_with_cache_for_model(
    upstream_port: u16,
    model_name: &str,
    slot_dir: PathBuf,
    slot_restore_supported: bool,
    pinned: bool,
) -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(FixedUpstream {
        port: upstream_port,
        model_name: model_name.into(),
        slot_restore_supported,
        pinned,
    });
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(TaggedCatalog {
        name: model_name.into(),
        tags: vec![],
        dialect: None,
    });
    let mcp = make_mcp_service();
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            Some(4096),
            runtime,
            catalog,
            mcp,
            cancel_clone,
            Arc::new(MockSettingsRepo),
            None, // inference_override
            None, // default_profile
            true, // cache_enabled
            Some(slot_dir),
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            std::sync::Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
            std::sync::Arc::new(gglib_core::domain::defects::ModelDefectLedger::new()),
            &gglib_core::ProxyAccessConfig::default(),
        )
        .await
        .ok();
    });

    // Give the proxy time to start listening.
    tokio::time::sleep(Duration::from_millis(50)).await;
    (format!("http://{}", addr), cancel)
}

/// [`spawn_proxy_with_cache_for_model`] with defaults matching the common
/// case: slot restore supported, not pinned (the `gglib proxy` shape).
pub(crate) async fn spawn_proxy_with_cache(
    upstream_port: u16,
    model_name: &str,
    slot_dir: PathBuf,
) -> (String, CancellationToken) {
    spawn_proxy_with_cache_for_model(upstream_port, model_name, slot_dir, true, false).await
}

/// [`spawn_proxy_with_cache`], pinned to `model_name` — the `gglib serve`
/// shape, as opposed to the default which models `gglib proxy`.
pub(crate) async fn spawn_pinned_proxy_with_cache(
    upstream_port: u16,
    model_name: &str,
    slot_dir: PathBuf,
) -> (String, CancellationToken) {
    spawn_proxy_with_cache_for_model(upstream_port, model_name, slot_dir, true, true).await
}

// ─── SSE frame assertions ──────────────────────────────────────────────────

/// Parse `data:` payloads from the SSE-encoded body. Returns one entry per
/// frame; `[DONE]` is tracked separately via the returned bool.
pub(crate) fn parse_sse_frames(body: &str) -> (Vec<Value>, bool) {
    let mut frames = Vec::new();
    let mut saw_done = false;
    for raw in body.split("\n\n") {
        let line = raw.trim_start();
        let Some(payload) = line.strip_prefix("data: ") else {
            continue;
        };
        if payload.trim() == "[DONE]" {
            saw_done = true;
            continue;
        }
        let v: Value = serde_json::from_str(payload).unwrap_or_else(|e| {
            panic!("proxy emitted non-JSON data frame: {e}\nframe: {payload}");
        });
        frames.push(v);
    }
    (frames, saw_done)
}

/// Assert that every frame has the OpenAI canonical envelope and a stable
/// `id` / `model` / `created` triple. Returns the (id, model, created).
pub(crate) fn assert_sse_canonical_envelope(
    frames: &[Value],
    expected_model: &str,
) -> (String, String, u64) {
    assert!(!frames.is_empty(), "expected at least one data frame");
    let first = &frames[0];
    let id = first["id"].as_str().expect("string id").to_owned();
    let model = first["model"].as_str().expect("string model").to_owned();
    let created = first["created"].as_u64().expect("u64 created");

    assert!(
        id.starts_with("chatcmpl-"),
        "id should start with chatcmpl-, got {id}"
    );
    assert_eq!(model, expected_model, "advertised model name mismatch");

    for f in frames {
        // PromptProgress frames are top-level (no choices) — they still must
        // share the envelope identity.
        assert_eq!(f["object"], "chat.completion.chunk");
        assert_eq!(f["id"], json!(id), "id must be stable across frames");
        assert_eq!(
            f["model"],
            json!(model),
            "model must be stable across frames"
        );
        assert_eq!(
            f["created"],
            json!(created),
            "created must be stable across frames"
        );
    }

    (id, model, created)
}
