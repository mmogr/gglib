//! Axum HTTP server for the OpenAI-compatible proxy.
//!
//! This module provides the `serve()` function that runs the proxy server
//! using a pre-bound `TcpListener` (from the supervisor).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::SystemTime;

use axum::{
    Json,
    extract::{State, rejection::BytesRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use gglib_core::Settings;
use gglib_core::cache_metrics::CacheMetricsStore;
use gglib_core::domain::InferenceConfig;
use gglib_core::domain::defects::LoopGuardTrip;
use gglib_core::ports::{Admission, ModelCatalogPort, ModelRuntimeError, ModelRuntimePort};
use gglib_core::ports::{AgentRunStarter, HubChatsPort, RemoteGatewayPort, RunsPort};
use gglib_core::request_pipeline::{ModelContext, ModelRoute, SamplingLayers, resolve_route};
use gglib_core::retry::RetryPolicy;
use gglib_mcp::McpService;

use crate::cache_lifecycle::{StreamConfig, resolve_cache_triple, run_with_cache};
use crate::connections::ActiveConnectionsRegistry;
use crate::dashboard::{CacheStatus, CacheStatusCache, DashboardState, spawn_dashboard_publisher};
use crate::forward::{ForwardError, ForwardRequest, forward_chat_completion};
use crate::mcp::session::SessionManager;
use crate::metrics::ContextMetricsStore;
use crate::models::{ChatRoutingEnvelope, ErrorResponse};
use crate::profiles::configured_names;
use crate::sampling_audit::SamplingAuditStore;
use crate::serve_config::ServeConfig;
use crate::slot_cache_state::SlotCacheState;
use crate::slots_poller::{SlotsCache, spawn_slots_poller};
use crate::token_calibration::TokenCalibration;
use crate::upstream_health::UpstreamHealth;
use crate::upstream_read::StreamBounds;
use gglib_core::services::SettingsCache;
use gglib_sse::SseOptions;

/// Shared application state for the proxy server.
#[derive(Clone)]
pub(crate) struct AppState {
    /// HTTP client for forwarding requests to llama-server.
    pub(crate) client: reqwest::Client,
    /// Port for managing model runtime.
    pub(crate) runtime_port: Arc<dyn ModelRuntimePort>,
    /// Port for listing and resolving models.
    pub(crate) catalog_port: Arc<dyn ModelCatalogPort>,
    /// MCP service for tool gateway.
    pub(crate) mcp: Arc<McpService>,
    /// Session manager for MCP Streamable HTTP sessions.
    pub(crate) sessions: SessionManager,
    /// Default context size when not specified in request.
    pub(crate) default_ctx: Option<u64>,
    /// Whether this machine's device memory can be read, and therefore whether
    /// a launch with nothing configured gets a context fitted to it.
    ///
    /// Supplied by the runtime, which owns the probe; this crate has none and
    /// cannot depend on the one that does. `false` on every AMD, Intel, Vulkan
    /// and CPU-only host, where `fit_context` refuses and the chain lands on
    /// the built-in floor — see [`crate::models_endpoint`] for what that means
    /// for the advertisement.
    pub(crate) device_memory_readable: bool,
    /// Unified proxy dashboard state: active-connections registry, llama.cpp
    /// `/slots` cache, and request metrics, plus the SSE broadcaster that
    /// pushes snapshots to `GET /v1/proxy/status/stream`.
    pub(crate) dashboard: Arc<DashboardState>,
    /// Application settings, snapshotted so the per-request read does not hit
    /// the database every time. See `settings_cache` module docs.
    pub(crate) settings: Arc<SettingsCache>,
    /// Fires when `serve` has been asked to shut down.
    ///
    /// Only long-lived responses need this, to end themselves instead of
    /// holding a connection open: `with_graceful_shutdown` waits for every
    /// in-flight connection to close, so an endless stream stops the server
    /// from ever returning. Request/response handlers ignore it entirely —
    /// they finish on their own and the drain takes care of them.
    pub(crate) shutdown: CancellationToken,
    /// Cancels the *daemon*, when this proxy is running under one.
    ///
    /// `None` for an embedded server or a test, where there is no daemon to
    /// stop and the remote shutdown route says so rather than pretending.
    daemon_shutdown: Option<CancellationToken>,
    /// The remote tunnel's owner, when this proxy may be reached through one.
    /// Asked to redeem a pairing code and whether `/mcp` is open to tunnelled
    /// requests; told when one arrives. `None` where no tunnel can exist.
    remote: Option<Arc<dyn RemoteGatewayPort>>,
    /// The daemon's runs, served at `/v1/runs`; `None` answers those 503.
    pub(crate) runs: Option<Arc<dyn RunsPort>>,
    /// The hub's chats, served at `/v1/chats`; `None` answers those 503.
    pub(crate) chats: Option<Arc<dyn HubChatsPort>>,
    /// Starts a device's turn on a hub chat; `None` answers it 503.
    pub(crate) turns: Option<Arc<dyn AgentRunStarter>>,
    /// Consecutive-failure watchdog: trips a proactive model recycle when the
    /// upstream degrades to empty responses / first-byte timeouts while still
    /// passing its `/health` check.
    upstream_health: Arc<UpstreamHealth>,
    /// How long a request may wait on a silent upstream, and a streamed reply
    /// on a client that stopped reading.
    pub(crate) stream_bounds: StreamBounds,
    /// Per-model chars-per-token calibration, learned from upstream usage
    /// frames and used to size the truncation budget.
    pub(crate) calibration: Arc<TokenCalibration>,
    /// Operator overrides from the command line, applied above the client's
    /// own request parameters when resolving sampling.
    inference_override: Option<gglib_core::domain::InferenceConfig>,
    default_profile: Option<String>,
    /// Whether the missing-default-profile warning has already been emitted
    /// this run, so it is said once rather than on every completion.
    default_profile_missing_logged: Arc<AtomicBool>,
    /// Whether KV cache persistence is enabled (opt-in via --cache).
    pub(crate) cache_enabled: bool,
    /// Resolved slot directory path (Some only when `cache_enabled`).
    pub(crate) slot_dir: Option<PathBuf>,
    /// Semaphore gating restore→forward→save cycles to prevent interleaving.
    slot_gate: Arc<Semaphore>,
    /// What is remembered of the slot cache between requests: the session hot
    /// in RAM, what has been cleared, and when the server now running started.
    pub(crate) slot_cache: Arc<SlotCacheState>,
    /// Where the loop guard records each decision and each scanned request,
    /// to outlive the process. `None` records nothing.
    pub(crate) loop_guard_trips: Option<Arc<dyn gglib_core::ports::LoopGuardTripSink>>,
}

impl AppState {
    /// The daemon's cancellation token, when this proxy runs under one.
    pub(crate) fn daemon_shutdown(&self) -> Option<CancellationToken> {
        self.daemon_shutdown.clone()
    }

    /// The remote tunnel's owner, when there is one.
    pub(crate) fn remote_gateway(&self) -> Option<Arc<dyn RemoteGatewayPort>> {
        self.remote.clone()
    }

    /// Build a [`StreamConfig`] for `base_url`/`model_id`, sharing this
    /// state's slot cache. Returns `None` when `slot_dir` isn't configured —
    /// the one condition under which a `StreamConfig` cannot be built, since
    /// it holds `slot_dir` as an owned (not `Option`) `PathBuf`.
    fn build_stream_config(&self, base_url: String, model_id: u32) -> Option<StreamConfig> {
        self.slot_dir.as_ref().map(|dir| StreamConfig {
            client: self.client.clone(),
            base_url,
            slot_dir: dir.clone(),
            model_id,
            state: Arc::clone(&self.slot_cache),
        })
    }
}

/// Start the proxy server on the pre-bound listener in `config`.
///
/// This function runs the Axum server until `config.cancel` is triggered.
/// [`ServeConfig`] documents each thing it is given.
///
/// # Returns
///
/// Returns `Ok(())` on clean shutdown, or an error if the server fails.
pub async fn serve(config: ServeConfig) -> anyhow::Result<()> {
    // Every field by name and no `..`: one this function never reads is an
    // unused binding, not a silently ignored setting.
    let ServeConfig {
        listener,
        default_ctx,
        device_memory_readable,
        runtime_port,
        catalog_port,
        mcp,
        cancel,
        daemon_cancel,
        settings_repo,
        inference_override,
        default_profile,
        cache_enabled,
        slot_dir,
        disk_budget,
        agent_metrics,
        observers,
        access,
    } = config;
    let addr = listener.local_addr()?;
    info!("Proxy server starting on {addr}");

    // Create HTTP client for upstream requests.
    //
    // We use connect_timeout (not a total-request timeout) deliberately:
    //
    // * A wall-clock `.timeout()` on the whole request kills long-running
    //   SSE streams — e.g. a 36 k-token prompt at 125 t/s takes ~290 s of
    //   prompt processing before the first generated token appears.  With a
    //   300 s total timeout, any heavy request races against that deadline
    //   and the proxy severs the connection mid-stream, surfacing a spurious
    //   "upstream SSE byte-stream error" to the client.
    //
    // * connect_timeout only measures the TCP handshake to 127.0.0.1, which
    //   completes in <1 ms under normal conditions.  A 10 s budget is more
    //   than enough to detect a dead/not-yet-started port while imposing no
    //   limit on how long an actual inference may take.
    //
    // Built by `loopback`, so it never goes through a proxy to reach 127.0.0.1.
    // A streamed reply is bounded by the drain instead, which times each read
    // of the body (`StreamBounds::idle`): a llama-server that crashes
    // mid-stream breaks the byte stream, and one that wedges with the socket
    // open is caught by that bound. `drain_events` documents what the client
    // is then sent. A request that does not stream is bounded as a whole
    // (`StreamBounds::unary`), around its send and the read of its body, in
    // `unary_body::exchange`.
    let client = crate::loopback::client_builder()
        .pool_max_idle_per_host(10)
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Background poller for llama.cpp's native `/slots` endpoint, feeding
    // the proxy dashboard's context-remaining display. It runs as its own
    // isolated Tokio task (see `slots_poller` module docs for the
    // backoff/lifecycle design) and is joined below after `axum::serve`
    // returns, so it never outlives the server or gets left detached.
    //
    // It also drives the Tier C sampling readback, which needs both stores
    // built before the task starts: the connections registry supplies the
    // intents in flight, and the audit store collects what the comparison
    // finds. Both are handed to `DashboardState` below as well, so the
    // dashboard reads exactly what the poller writes.
    let slots_cache = Arc::new(SlotsCache::new());
    let connections = Arc::new(ActiveConnectionsRegistry::new());
    let sampling_audit = Arc::new(SamplingAuditStore::new());
    let slots_poller = spawn_slots_poller(
        Arc::clone(&runtime_port),
        client.clone(),
        Arc::clone(&slots_cache),
        Arc::clone(&connections),
        Arc::clone(&sampling_audit),
        cancel.clone(),
    );

    // Upstream-degradation watchdog, shared between the request path (strike
    // recording + recycle) and the dashboard (counter surfacing).
    let upstream_health = Arc::new(UpstreamHealth::new());

    // Shared cache state (constructed once, shared across all requests).
    // Always initialized; the `cache_enabled` guard prevents acquire() when disabled.
    let slot_gate = Arc::new(Semaphore::new(1));

    // Background byte-budget eviction, so cached session slot files don't
    // accumulate without bound. Only runs when there's a slot_dir to sweep;
    // joined below on shutdown like the other background tasks.
    let lru_eviction = slot_dir.as_ref().map(|dir| {
        crate::slot_eviction::spawn_eviction_task(dir.clone(), disk_budget, cancel.clone())
    });

    let dashboard = Arc::new(DashboardState::new(
        connections,
        slots_cache,
        Arc::new(ContextMetricsStore::new().with_ledger(observers.defects)),
        Arc::clone(&upstream_health),
        Arc::new(CacheStatusCache::new()),
        Arc::new(CacheMetricsStore::new()),
        agent_metrics,
        Arc::clone(&runtime_port),
        sampling_audit,
    ));
    // Second background task: periodically recomputes and broadcasts the
    // unified DashboardSnapshot for GET /v1/proxy/status/stream subscribers
    // (see `dashboard` module docs). Same join-on-shutdown treatment as the
    // slots poller above.
    let dashboard_publisher = spawn_dashboard_publisher(Arc::clone(&dashboard), cancel.clone());

    let state = AppState {
        client,
        runtime_port,
        catalog_port,
        mcp,
        sessions: SessionManager::new(),
        default_ctx,
        device_memory_readable,
        dashboard,
        settings: Arc::new(SettingsCache::new(settings_repo)),
        shutdown: cancel.clone(),
        daemon_shutdown: daemon_cancel,
        remote: access.remote.clone(),
        runs: access.devices.runs.clone(),
        chats: access.devices.chats.clone(),
        turns: access.devices.turns.clone(),
        upstream_health,
        stream_bounds: StreamBounds::for_serve(),
        calibration: Arc::new(TokenCalibration::new()),
        inference_override,
        default_profile,
        default_profile_missing_logged: Arc::new(AtomicBool::new(false)),
        cache_enabled,
        slot_dir,
        slot_gate,
        // Until an admission reports a fresh server, slot files older than
        // this proxy are taken to be an earlier server's.
        slot_cache: Arc::new(SlotCacheState::new(SystemTime::now())),
        loop_guard_trips: observers.loop_guard_trips,
    };

    let app = crate::router::build(state, &access);

    info!("Proxy listening on {addr}");
    info!("Configure OpenWebUI to use: http://{addr}/v1");
    info!("MCP Streamable HTTP endpoint: http://{addr}/mcp");

    axum::serve(listener, app)
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await?;

    // Ensure both background tasks are fully joined (not just cancelled-
    // and-detached) before `serve()` returns, so callers can rely on a
    // clean shutdown leaving no dangling tasks behind.
    if let Err(e) = slots_poller.await {
        warn!("proxy dashboard: /slots poller task panicked during shutdown: {e}");
    }
    if let Err(e) = dashboard_publisher.await {
        warn!("proxy dashboard: publisher task panicked during shutdown: {e}");
    }
    if let Some(handle) = lru_eviction
        && let Err(e) = handle.await
    {
        warn!("proxy cache: LRU eviction task panicked during shutdown: {e}");
    }

    info!("Proxy server shut down");
    Ok(())
}

/// Health check endpoint.
pub(crate) async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok"
    }))
}

/// Return the unified proxy dashboard snapshot: active connections,
/// llama.cpp `/slots` state, and recent request metrics.
///
/// This is the shared data contract for the CLI TUI and web dashboard.
pub(crate) async fn handle_proxy_status(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.dashboard.snapshot())
}

/// Subscribe to a live stream of [`crate::dashboard::DashboardSnapshot`]
/// updates via Server-Sent Events.
///
/// Uses hydrate-then-stream semantics (via [`gglib_sse::Broadcaster`]): the
/// client immediately receives one event carrying the current snapshot,
/// then a fresh snapshot on every subsequent publish tick — no waiting for
/// the next tick to see the current state.
pub(crate) async fn handle_proxy_status_stream(State(state): State<AppState>) -> impl IntoResponse {
    let current = state.dashboard.snapshot();
    // Bounded by the shutdown token: this stream is the one response on the
    // proxy that would otherwise never end, and `with_graceful_shutdown` waits
    // for every connection to close before `serve` returns. Left unbounded, a
    // single dashboard subscriber — the tray panel keeps one open the whole
    // time the proxy runs — stops the proxy from ever stopping cleanly, until
    // the supervisor gives up and aborts the task.
    Arc::clone(&state.dashboard.broadcaster).subscribe_with_hydration_until(
        current,
        SseOptions::default(),
        state.shutdown.clone().cancelled_owned(),
    )
}

/// Handle chat completions - ensure model is running and proxy to llama-server.
pub(crate) async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    debug!("POST /v1/chat/completions");
    let body = match body {
        Ok(body) => body,
        Err(rejection) => return crate::body_limit::rejected(rejection),
    };

    // Canonicalize the system prompt and tool order once, up front, and
    // reuse the result for both the content-hash session id fallback below
    // and the forwarded request (forward_chat_completion does not
    // re-canonicalize) — avoids paying the parse/regex/serialize cost on
    // this ~150KB+ body twice per request.
    let body = crate::canonicalization::canonicalize_system_prompt(body);
    let body = crate::canonicalization::canonicalize_tool_order(body);

    // Extract and sanitize session ID from header (safety-critical: prevents path traversal)
    let session_id_from_header = headers
        .get("x-gglib-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let sanitized_session_id = if let Some(ref sid) = session_id_from_header {
        match crate::slots::sanitize_session_id(sid) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!("Invalid session ID in header: {}", e);
                return Response::builder()
                    .status(axum::http::StatusCode::BAD_REQUEST)
                    .body(axum::body::Body::from(format!("Invalid session ID: {e}")))
                    .unwrap();
            }
        }
    } else {
        // No explicit header — most clients (VS Code Copilot's LLM Gateway
        // extension, curl, anything else speaking plain OpenAI-compatible
        // chat completions) have no idea X-Gglib-Session-Id exists. Derive a
        // stable fallback from the request content itself so the cache
        // still works without any client cooperation.
        //
        // Derived unconditionally — not gated on `state.cache_enabled` — because
        // this id also keys `TokenCalibration`'s per-session budget
        // snapshot (see `forward_chat_completion`'s `calibration_session_id`),
        // which must work even when disk KV-slot caching is off. That's
        // exactly the case for hybrid/sliding-window-attention models, where
        // disk restore can't resume the prompt and is disabled by design (see
        // `slot_restore` in `gglib_runtime::llama::args`) — but the host-RAM
        // prompt cache the frozen budget protects still applies. The
        // actual disk save/restore activation stays independently gated on
        // `state.cache_enabled` at its own call site below, so deriving the
        // id here doesn't turn on disk caching when the feature is off.
        crate::fallback_session::derive_fallback_session_id(&body)
    };

    if let Some(ref sid) = sanitized_session_id {
        debug!(
            session_id = %sid,
            source = if session_id_from_header.is_some() { "header" } else { "content-hash" },
            "resolved cache session id"
        );
        crate::canonicalization::log_tool_names_for_diagnostics(&body, sid);
    }

    // Extract the three routing fields from the request body.
    // ChatRoutingEnvelope only captures `model`, `stream`, and `num_ctx`;
    // all other fields are ignored by serde and the raw bytes are forwarded
    // unchanged. This makes the proxy immune to content-array messages,
    // stop as a bare string, and any future OpenAI request extensions.
    let envelope: ChatRoutingEnvelope = match serde_json::from_slice(&body) {
        Ok(env) => env,
        Err(e) => {
            error!("Failed to parse request: {e}");
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::invalid_request(&format!(
                    "Invalid request body: {e}"
                ))),
            )
                .into_response();
        }
    };

    let model_name = envelope.model.clone();
    let is_streaming = envelope.stream;
    let num_ctx = envelope.num_ctx;

    info!(
        model = %model_name,
        streaming = %is_streaming,
        num_ctx = ?num_ctx,
        "Processing chat completion request"
    );

    // One settings view for the whole request: the profile list read here and
    // the global defaults read further down come from the same snapshot, so a
    // concurrent settings edit cannot apply to half a request.
    let settings = state.settings.get().await;
    let configured_profiles = settings.inference_profiles.as_deref().unwrap_or_default();

    // Resolve any `{model}:{profile}` suffix. Everything downstream — the
    // model launch, dashboard registration, metrics, cache keys — uses the
    // model the base resolves to, so a profile never causes a second model to
    // be launched.
    let (model_name, request_profile) = match resolve_route(
        &model_name,
        configured_profiles,
        state.catalog_port.as_ref(),
    )
    .await
    {
        // A bare name takes the endpoint's default profile, if one was set.
        // Applied here, once, rather than where `attempt` builds the
        // `SamplingLayers` of each attempt.
        //
        // A default whose profile has since been deleted degrades to
        // unprofiled rather than 404ing: the client never named it and cannot
        // fix it. An explicitly named suffix still fails loudly — see the
        // `ProfileNotFound` arm.
        //
        // `Bare` is also route case 5 — nothing resolved — so an id like
        // `no-such-model:typo` reaches here and would take the default. It is
        // contained because case 5 returns the id *with its suffix intact*
        // (`profile_route`'s final arm), and that id then fails model
        // resolution. The guarantee therefore lives in the router, not in
        // admission: a change there that stripped the suffix in case 5 would
        // open this path, so it is named here rather than assumed.
        ModelRoute::Bare(model) => {
            let default = state.default_profile.as_deref().and_then(|name| {
                let found = configured_profiles.iter().find(|p| p.name == name);
                // Once per run, not once per request: the operator needs to be
                // told, and a WARN on every completion for the life of the run
                // buries the rest of the log. Same discipline as the
                // restart-detection path below.
                if found.is_none()
                    && !state
                        .default_profile_missing_logged
                        .swap(true, AtomicOrdering::Relaxed)
                {
                    warn!(
                        profile = %name,
                        "the endpoint's default profile is no longer configured; \
                         serving bare-model requests unprofiled until it is restored"
                    );
                }
                found
            });
            (model.to_owned(), default.map(|p| p.config.clone()))
        }
        ModelRoute::Profiled { model, profile } => (model.to_owned(), Some(profile.config.clone())),
        ModelRoute::ProfileNotFound { requested, suffix } => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::profile_not_found(
                    requested,
                    suffix,
                    configured_names(configured_profiles).as_deref(),
                )),
            )
                .into_response();
        }
    };

    // The model this request is for, from here on. A client may name it by id
    // or by name; resolving once, here, is what makes `3` and `qwen` one model
    // to the loop guard, the pin, the dashboard, calibration and the echo. An
    // unknown model is refused now, as `model_not_found`, before any guard or
    // swap. A bare name costs two catalog reads in all: this one and
    // admission's launch lookup (a `:profile` suffix adds routing's above).
    let model = match gglib_core::request_pipeline::resolve_summary(
        state.catalog_port.as_ref(),
        &model_name,
    )
    .await
    {
        Ok(model) => model,
        Err(e) => return refuse_unresolved(&model_name, e),
    };
    let model_context = gglib_core::request_pipeline::ModelContext::from(&model);

    // The turn-level loop/stagnation guard, keyed by the model the request
    // resolved to. Under `refuse` it answers here, after the one catalog read
    // above but before admission or any model swap is paid for; under `note`
    // the request goes on and carries the note and the trip with it. See
    // `loop_guard_step` for what it records and `loop_guard` for what a
    // replayed history means.
    let mut guard_note = None;
    let mut loop_guard_trip = None;
    let guard_observers = crate::loop_guard_step::GuardObservers {
        metrics: &state.dashboard.metrics,
        trips: state.loop_guard_trips.as_deref(),
        session_id: sanitized_session_id.as_deref(),
    };
    match crate::loop_guard_step::run(&settings, &body, &model.name, &guard_observers) {
        crate::loop_guard_step::GuardStep::Forward => {}
        crate::loop_guard_step::GuardStep::Note { note, trip } => {
            guard_note = Some(note);
            loop_guard_trip = Some(trip);
        }
        crate::loop_guard_step::GuardStep::Refuse(response) => return response,
    }

    // Watchdog: if prior requests asked for a recycle (a strike streak, or a
    // stall after the first token, while the server still passes /health),
    // carry it out now — before routing this request into a server that has
    // proven it is not producing output.
    recycle_if_asked_and_idle(&state).await;

    // An embedding model cannot answer this. gglib launches models tagged
    // `embedding` with `--embeddings`, which llama-server reads as "restrict to
    // only the embedding use case" — that server refuses chat completions
    // outright. Forwarding anyway would evict whatever is currently serving
    // chat, load the embedding model, and collect a 501, leaving the endpoint
    // worse off than before the request arrived.
    if model
        .tags
        .iter()
        .any(|t| t == crate::embeddings::EMBEDDING_TAG)
    {
        info!(
            model = %model.name,
            "refusing chat completion for an embedding-only model"
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::embedding_model_cannot_chat(&model.name)),
        )
            .into_response();
    }

    // Nor can a model with no projector read an image, in this turn or in
    // the history: refused here by name, before a swap is paid for.
    if let Some(refusal) = crate::image_refusal::refuse_images(&model, &body) {
        return refusal;
    }

    // Join the admission queue.
    //
    // This is where a request for a model that is not loaded waits — batched
    // with every other request for the same model, so one swap serves all of
    // them rather than each paying for its own. A slow 200 beats a fast 503 for
    // an OpenAI-compatible client, which treats 503 as terminal (see the
    // UpstreamDead path below, which already avoids 503 for that reason).
    //
    // `admission.lease` is held for the whole of this request — `attempt`
    // moves it onto the connection guard — and is what stops the model being
    // swapped out from under a response that is still streaming.
    //
    // Admitted by id, so the runtime serves and pins the model resolved above
    // rather than whichever row a name finds first.
    let model_id = model.id.to_string();
    let admit = async || {
        let overrides = gglib_core::ports::LaunchOverrides::default();
        state
            .runtime_port
            .admit(&model_id, num_ctx, state.default_ctx, overrides)
            .await
    };
    let mut admission = match admit().await {
        Ok(admission) => admission,
        Err(e) => return handle_runtime_error(e),
    };
    // Ask the watchdog again now that this request is through the queue. It
    // may have waited there behind a turn that stalled and asked for a recycle
    // as it ended, after the check above had found that turn in flight; left
    // alone, it would be forwarded to the server just condemned. The stop runs
    // under this request's lease: with one request per server
    // (`--parallel 1`), no other request can be admitted onto the server this
    // one was admitted to until the lease goes back, which is after the stop
    // and before the request queues again.
    if state.upstream_health.recycle_pending() && state.dashboard.connections.is_empty() {
        recycle_if_asked_and_idle(&state).await;
        drop(admission);
        admission = match admit().await {
            Ok(admission) => admission,
            Err(e) => return handle_runtime_error(e),
        };
    }

    // The note goes on here, before the body is handed to an attempt, so every
    // path that derives from it carries it: both attempts, the unary path and
    // the repair re-issue. After the scan above, so it can never trip the
    // guard that wrote it.
    let body = match guard_note {
        Some(note) => note.append_to(body),
        None => body,
    };

    let ctx = RequestCtx {
        headers: &headers,
        is_streaming,
        session_id: sanitized_session_id.as_deref(),
        profile: request_profile,
        context: model_context,
    };

    // The first attempt gets a clone of the body, so the original is still in
    // hand if the upstream turns out to be dead. `Bytes` is reference-counted,
    // so the clone is O(1).
    let first = body.clone();
    match attempt(&state, &ctx, admission, &settings, first, loop_guard_trip).await {
        Ok(response) => response,
        Err(ForwardError::UpstreamDead) => {
            // llama-server was dead after admission returned a stale port.
            // Strategy:
            //   1. Clear stale state via stop_current().
            //   2. Re-admit — the queue does the waiting, so one request
            //      drives the restart and concurrent requests are batched
            //      behind it rather than surfacing a 503 to the client (the VS
            //      Code LLM Gateway treats 503 as a terminal error).
            //   3. Run the attempt again, once, against the new admission.
            warn!(
                model = %model.name,
                "upstream dead — clearing stale state and restarting model for transparent retry"
            );
            let _ = state.runtime_port.stop_current().await;

            // AdmissionTimeout is deliberately not retried here: it means the
            // GPU is oversubscribed rather than that this model is still
            // loading, so it falls through to a 503 + Retry-After and the
            // client controls its own backoff.
            let admission = match admit().await {
                Ok(admission) => admission,
                Err(e) => return handle_runtime_error(e),
            };
            // Settings are read again: the model was just relaunched, so this
            // is a fresh point in time.
            let settings = state.settings.get().await;
            // No trip. The first attempt already recorded it, and the ledger
            // counts one intervention per request the guard acted on, not one
            // per attempt. (`requests` is counted per attempt on this path.)
            // The retry does carry the note, which is in `body`.
            match attempt(&state, &ctx, admission, &settings, body, None).await {
                Ok(response) => response,
                // Dead again straight after a restart: give up, with the 503
                // and the Retry-After a model that is still loading gets.
                Err(ForwardError::UpstreamDead) => {
                    handle_runtime_error(ModelRuntimeError::ModelLoading)
                }
            }
        }
    }
}

/// What one chat completion carries into each of its attempts: the parts no
/// admission and no settings read can change.
struct RequestCtx<'a> {
    headers: &'a HeaderMap,
    is_streaming: bool,
    /// The sanitized session id, from the header or the content hash.
    session_id: Option<&'a str>,
    /// The profile the request's model id resolved to. A retry does not
    /// resolve it again: the client asked for a specific one.
    profile: Option<InferenceConfig>,
    /// The model's stored capabilities, read once before admission. A retry
    /// follows a restart of the same model, so reading the catalog again
    /// could only return what is already in hand.
    context: ModelContext,
}

/// Forward `body` to the server `admission` names: one attempt at the request
/// `ctx` describes.
///
/// A request's first attempt and the one after a dead upstream's restart are
/// both this function, so neither can do a step the other leaves out. They
/// differ only in what they are handed: the admission, the `settings` read at
/// the time, and `trip`, which the first alone carries.
///
/// # Errors
///
/// [`ForwardError::UpstreamDead`] when the admitted server could not be
/// reached.
async fn attempt(
    state: &AppState,
    ctx: &RequestCtx<'_>,
    admission: Admission,
    settings: &Settings,
    body: Bytes,
    trip: Option<LoopGuardTrip>,
) -> Result<Response, ForwardError> {
    let Admission { target, lease } = admission;
    // Every key from here on — the connection, the forward, calibration, the
    // dashboard's per-model rows and the SSE echo — is the admitted model's
    // own name, never the string the request happened to spell it with.
    let model_name = target.model_name.as_str();

    // A freshly started server holds nothing in RAM, and the slot files on
    // disk are an earlier server's. See `SlotCacheState::on_restart`.
    if target.just_started {
        state.slot_cache.on_restart(SystemTime::now());
    }

    // Build upstream URL
    let upstream_url = format!("{}/v1/chat/completions", target.base_url);
    debug!(
        upstream = %upstream_url,
        model_id = %target.model_id,
        model_name = %target.model_name,
        "Routing to llama-server"
    );

    // Record how caching resolved for this model. Written here rather than at
    // launch because the dashboard lives in this crate and the launch decision
    // lives in the runtime — the target is where the two meet. Cheap and
    // idempotent: `set` skips the write when nothing changed, which is every
    // request after the first for a given model.
    state.dashboard.cache.set(CacheStatus::build(
        state.cache_enabled && state.slot_dir.is_some(),
        target.slot_restore_supported,
        target.cache_ram_health,
    ));

    // Same rationale, same meeting point: the launch decided all of this in
    // the runtime, and this is where the dashboard first sees the result.
    if let Some(narration) = target.narration.clone() {
        state.dashboard.launch.set(narration);
    }

    // Register this attempt in the active-connections dashboard registry.
    // The returned guard unregisters on drop (see `connections` module docs)
    // — normal completion, early return, client disconnect, or panic all
    // clean up without any explicit unregister call at each exit point. An
    // attempt that finds the upstream dead drops it on the way out, and its
    // admission lease with it.
    //
    // The admission lease rides along on the guard (see `connections` module
    // docs): it must outlive the response, including across the streaming
    // path's spawned task, and the guard already goes exactly that far.
    let connection = state
        .dashboard
        .connections
        .register(
            model_name.to_owned(),
            ctx.is_streaming,
            Some(target.effective_ctx),
        )
        .holding(lease);

    // Global defaults come from `settings`: on a first attempt, the same
    // snapshot the profile list did.
    let sampling = SamplingLayers {
        cli_override: state.inference_override.clone(),
        profile: ctx.profile.clone(),
        global: settings.inference_defaults.clone(),
        trust_client_sampling: settings.trust_client_sampling.unwrap_or(false),
        // Opt-out: absent means on. See `Settings::agentic_sampling`.
        agentic_adjustments: settings.agentic_sampling != Some(false),
    };

    // Build StreamConfig for this attempt (Some only when cache is enabled).
    //
    // `slot_restore_supported` is false for sliding-window/hybrid/recurrent
    // models, where a disk restore cannot resume the prompt and actively
    // suppresses the in-RAM prompt cache that would have (see
    // `gglib_runtime::llama::args::slot_restore`). Leaving the config `None`
    // takes every disk save/restore call out of the request path; the
    // host-RAM cache handles conversation switching by itself.
    let stream_config = if state.cache_enabled && target.slot_restore_supported {
        state.build_stream_config(target.base_url.clone(), target.model_id)
    } else {
        None
    };

    // Everything forward_chat_completion needs that doesn't vary across the
    // cache-branching below — see `ForwardRequest` docs.
    let req = ForwardRequest {
        repair_enabled: settings.tool_call_repair != Some(false),
        client: &state.client,
        upstream_url: &upstream_url,
        headers: ctx.headers,
        body,
        is_streaming: ctx.is_streaming,
        model_name,
        effective_ctx: target.effective_ctx,
        context: ctx.context.clone(),
        metrics: state.dashboard.metrics.clone(),
        sampling,
        connection,
        upstream_health: state.upstream_health.clone(),
        stream_bounds: state.stream_bounds,
        calibration: state.calibration.clone(),
        calibration_session_id: ctx.session_id,
        cache_metrics: state.dashboard.cache_metrics.clone(),
        sampling_audit: state.dashboard.sampling_audit.clone(),
        loop_guard_trip: trip,
    };

    // Forward the request, optionally wrapped in cache lifecycle. `Some(cfg)`
    // in `stream_config` already implies `state.cache_enabled` (see its
    // construction just above), so matching on `(session_id, stream_config)`
    // alone — without a redundant outer `cache_enabled` check — covers every
    // case: cache disabled, cache enabled but no session id/config, and
    // cache enabled with both all fall into the same "no triple" arm below.
    //
    // Every arm can return `UpstreamDead`: the streaming one from its TCP
    // probe, the others from the send.
    match (ctx.session_id, &stream_config) {
        (Some(sid), Some(cfg)) => {
            if ctx.is_streaming {
                // Streaming with cache: use prepare_streaming_cycle + sse_stream::spawn_and_return
                let (permit, cfg, sid) =
                    resolve_cache_triple(cfg, state.slot_gate.clone(), sid).await;
                forward_chat_completion(req, permit, cfg, sid).await
            } else {
                // Non-streaming with cache: wrap in run_with_cache (fail-open internally)
                let (resp, _restore_result) = run_with_cache(cfg, &state.slot_gate, sid, || {
                    forward_chat_completion(req, None, None, None)
                })
                .await
                .expect(
                    "run_with_cache only returns Err on sanitization failure, which is already checked",
                );
                resp
            }
        }
        // Cache disabled, or cache enabled but no session id/config: direct call
        _ => forward_chat_completion(req, None, None, None).await,
    }
}

/// Recycle the model if the watchdog asked for it and nothing is in flight.
///
/// "Nothing in flight" is read from the connection registry, so it means
/// something only before the calling request registers its own connection.
/// With `--parallel 1` a request in flight owns the only slot, and a stop
/// would kill its live generation. The `&&` leaves the request untaken when
/// busy, for the next request that finds the upstream idle. An agent run is
/// not in the registry; it holds the model instead, and `recycle_current`
/// refuses while it does, so the request is re-armed for later (#1212).
async fn recycle_if_asked_and_idle(state: &AppState) {
    if !(state.dashboard.connections.is_empty() && state.upstream_health.take_recycle_request()) {
        return;
    }
    warn!("upstream watchdog: recycling degraded model before next request");
    // Taking the request already cleared the flag and zeroed the streak, so a
    // swallowed failure here spends the watchdog's entire case against a
    // server that is still sick. Put it back instead. With gglib's runtime the
    // failure is a run's hold: a kill that fails is logged, the slot emptied.
    if let Err(e) = state.runtime_port.recycle_current().await {
        warn!(
            error = %e,
            "upstream watchdog: recycle failed; re-arming for the next idle request"
        );
        state.upstream_health.rearm_recycle();
    }
}

/// Header naming *why* a 503 was returned, so a client or dashboard can tell an
/// oversubscribed admission queue from ordinary model loading — the two are
/// identical on the wire otherwise, since both serialise to
/// `service_unavailable`.
const RETRY_REASON_HEADER: &str = "x-gglib-retry-reason";

/// Value of [`RETRY_REASON_HEADER`] when the admission queue timed the request
/// out.
const RETRY_REASON_ADMISSION: &str = "admission";

/// Answer a request whose model did not resolve: `model_not_found` for a model
/// the catalog does not hold, and a 500 that is also logged for a catalog that
/// could not be read, which means something is broken.
pub(crate) fn refuse_unresolved(model: &str, err: ModelRuntimeError) -> Response {
    if matches!(err, ModelRuntimeError::Internal(_)) {
        error!(model = %model, error = %err, "catalog lookup failed");
    }
    handle_runtime_error(err)
}

/// Convert `ModelRuntimeError` to HTTP response with appropriate status code.
pub(crate) fn handle_runtime_error(err: ModelRuntimeError) -> Response {
    let status = StatusCode::from_u16(err.suggested_status_code())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let queued_out = matches!(err, ModelRuntimeError::AdmissionTimeout(_));
    let error_response = ErrorResponse::from(err);

    let mut response = (status, Json(error_response)).into_response();

    if status == StatusCode::SERVICE_UNAVAILABLE {
        // Derived from the shared policy rather than hardcoded, so the hint we
        // advertise cannot drift from the backoff our own clients apply.
        //
        // `max_backoff` rather than `initial_backoff`: by the time an admission
        // 503 escapes, the request has already sat in the queue for minutes, so
        // the oversubscription is plainly not clearing quickly. Advertising the
        // policy's *ceiling* — the longest single delay it would ever produce —
        // tells honest clients to come back at a sensible remove. Advertising
        // the opening delay instead would invite everyone who honours the header
        // back within a second or two, all at once and none of them jittered.
        let hint = RetryPolicy::default().max_backoff.as_secs().max(1);
        if let Ok(value) = hint.to_string().parse() {
            response
                .headers_mut()
                .insert(axum::http::header::RETRY_AFTER, value);
        }
        if queued_out && let Ok(value) = RETRY_REASON_ADMISSION.parse() {
            response.headers_mut().insert(RETRY_REASON_HEADER, value);
        }
    }

    response
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
