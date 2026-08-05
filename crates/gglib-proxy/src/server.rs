//! Axum HTTP server for the OpenAI-compatible proxy.
//!
//! This module provides the `serve()` function that runs the proxy server
//! using a pre-bound TcpListener (from the supervisor).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bytes::Bytes;
use reqwest::Client;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing::{debug, error, info, warn};

use gglib_core::cache_metrics::CacheMetricsStore;
use gglib_core::ports::{
    ModelCatalogPort, ModelRuntimeError, ModelRuntimePort, SettingsRepository,
};
use gglib_core::request_pipeline::SamplingLayers;
use gglib_core::retry::RetryPolicy;
use gglib_core::{CorsConfig, ProxyAccessConfig};
use gglib_mcp::McpService;

use crate::cache_lifecycle::{StreamConfig, clear_cache, resolve_cache_triple, run_with_cache};
use crate::connections::ActiveConnectionsRegistry;
use crate::dashboard::{CacheStatus, CacheStatusCache, DashboardState, spawn_dashboard_publisher};
use crate::forward::{ForwardError, ForwardRequest};
use crate::mcp::handlers::{delete_mcp, get_mcp, post_mcp};
use crate::mcp::session::SessionManager;
use crate::metrics::ContextMetricsStore;
use crate::models::{ChatRoutingEnvelope, ErrorResponse, ModelsResponse};
use crate::profiles::{ModelRoute, configured_names, resolve_route, variant_entries};
use crate::settings_cache::SettingsCache;
use crate::slots_poller::{SlotsCache, spawn_slots_poller};
use crate::token_calibration::TokenCalibration;
use crate::upstream_health::UpstreamHealth;
use dashmap::DashSet;
use gglib_sse::SseOptions;

/// Shared application state for the proxy server.
#[derive(Clone)]
pub(crate) struct AppState {
    /// HTTP client for forwarding requests to llama-server.
    pub(crate) client: Client,
    /// Port for managing model runtime.
    pub(crate) runtime_port: Arc<dyn ModelRuntimePort>,
    /// Port for listing and resolving models.
    pub(crate) catalog_port: Arc<dyn ModelCatalogPort>,
    /// MCP service for tool gateway.
    pub(crate) mcp: Arc<McpService>,
    /// Session manager for MCP Streamable HTTP sessions.
    pub(crate) sessions: SessionManager,
    /// Default context size when not specified in request.
    pub(crate) default_ctx: u64,
    /// Unified proxy dashboard state: active-connections registry, llama.cpp
    /// `/slots` cache, and request metrics, plus the SSE broadcaster that
    /// pushes snapshots to `GET /v1/proxy/status/stream`. Replaces what were
    /// previously three separate `AppState` fields (`metrics`, `connections`,
    /// `slots`) — see `dashboard` module docs for the consolidation rationale.
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
    shutdown: CancellationToken,
    /// Consecutive-failure watchdog: trips a proactive model recycle when the
    /// upstream degrades to empty responses / first-byte timeouts while still
    /// passing its `/health` check.
    upstream_health: Arc<UpstreamHealth>,
    /// Per-model chars-per-token calibration, learned from upstream usage
    /// frames and used to size the truncation budget.
    calibration: Arc<TokenCalibration>,
    /// Operator overrides from the command line, applied above the client's
    /// own request parameters when resolving sampling.
    inference_override: Option<gglib_core::domain::InferenceConfig>,
    /// Whether KV cache persistence is enabled (opt-in via --cache).
    pub(crate) cache_enabled: bool,
    /// Resolved slot directory path (Some only when cache_enabled).
    pub(crate) slot_dir: Option<PathBuf>,
    /// Semaphore gating restore→forward→save cycles to prevent interleaving.
    slot_gate: Arc<Semaphore>,
    /// When true, all pending saves are skipped (set on restart or explicit clear).
    clear_all_pending: Arc<AtomicBool>,
    /// Sessions that have been explicitly cleared (skip save for these).
    per_session_cleared: Arc<DashSet<String>>,
    /// Unix timestamp (seconds) when the current llama-server process started.
    /// Updated on each restart detection. Used by mtime guard to skip stale slots.
    server_start_time: Arc<AtomicU64>,
    /// Last session successfully loaded into RAM (hot in KV cache).
    /// Composite key (model_id + session_id) used to bypass disk restore
    /// when the same model+session is already hot.
    last_loaded_session:
        Arc<tokio::sync::RwLock<Option<crate::cache_lifecycle::LastLoadedSession>>>,
}

impl AppState {
    /// Build a [`StreamConfig`] for `base_url`/`model_id`, sourced from this
    /// state's cache-lifecycle fields. Returns `None` when `slot_dir` isn't
    /// configured — the one condition under which a `StreamConfig` cannot be
    /// built, since it holds `slot_dir` as an owned (not `Option`) `PathBuf`.
    fn build_stream_config(&self, base_url: String, model_id: u32) -> Option<StreamConfig> {
        self.slot_dir.as_ref().map(|dir| StreamConfig {
            client: self.client.clone(),
            base_url,
            slot_dir: dir.clone(),
            model_id,
            clear_all_pending: self.clear_all_pending.clone(),
            per_session_cleared: self.per_session_cleared.clone(),
            server_start_time: self.server_start_time.clone(),
            last_loaded_session: self.last_loaded_session.clone(),
        })
    }
}

/// Start the proxy server with a pre-bound listener.
///
/// This function runs the Axum server until the cancellation token is triggered.
///
/// # Arguments
///
/// * `listener` - Pre-bound TCP listener (from supervisor)
/// * `default_ctx` - Default context size for models
/// * `runtime_port` - Port for managing model runtime
/// * `catalog_port` - Port for listing and resolving models
/// * `mcp` - MCP service for tool gateway
/// * `cancel` - Cancellation token for graceful shutdown
/// * `settings_repo` - Settings repository, wrapped in a `SettingsCache` so the
///   per-request read is served from a short-lived snapshot rather than a query
/// * `disk_budget` - Byte budget for the on-disk slot cache eviction sweep.
///   Only consulted when `slot_dir` is `Some`.
/// * `agent_metrics` - Agent-path prompt-cache reuse store (GUI + CLI chat),
///   surfaced on the dashboard as `agent_usage` alongside the proxied figure.
///
/// # Returns
///
/// Returns `Ok(())` on clean shutdown, or an error if the server fails.
/// Build CORS layer from configuration.
fn build_cors_layer(config: &CorsConfig) -> CorsLayer {
    match config {
        CorsConfig::AllowAll => CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any),
        CorsConfig::AllowOrigins(origins) => {
            use axum::http::HeaderValue;
            let allowed: Vec<HeaderValue> = origins.iter().filter_map(|o| o.parse().ok()).collect();
            CorsLayer::new()
                .allow_origin(allowed)
                .allow_methods(Any)
                .allow_headers(Any)
        }
        CorsConfig::LocalOnly => {
            let local = AllowOrigin::predicate(|origin: &axum::http::HeaderValue, _req_headers| {
                gglib_core::is_local_origin(origin.to_str().unwrap_or(""))
            });
            CorsLayer::new()
                .allow_origin(local)
                .allow_methods(Any)
                .allow_headers(Any)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn serve(
    listener: TcpListener,
    default_ctx: u64,
    runtime_port: Arc<dyn ModelRuntimePort>,
    catalog_port: Arc<dyn ModelCatalogPort>,
    mcp: Arc<McpService>,
    cancel: CancellationToken,
    settings_repo: Arc<dyn SettingsRepository>,
    // Operator overrides from this process's command line, applied above the
    // client's own request parameters. See `SamplingLayers::cli_override`.
    inference_override: Option<gglib_core::domain::InferenceConfig>,
    cache_enabled: bool,
    slot_dir: Option<PathBuf>,
    disk_budget: crate::slot_eviction::DiskBudget,
    // Agent-path prompt-cache reuse store, owned by the supervisor so it can
    // also be shared with the embedded axum server (GUI chat) and outlives a
    // single proxy run. Exposed on the dashboard as `agent_usage`, alongside
    // the proxied figure.
    agent_metrics: Arc<CacheMetricsStore>,
    // Who may reach this endpoint: the CORS policy, the optional bearer token,
    // and the Host allowlist. Carries the `CorsConfig` it replaced rather than
    // sitting beside it — `serve` was already at fifteen parameters, and access
    // decisions belong together anyway.
    access: &ProxyAccessConfig,
) -> anyhow::Result<()> {
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
    // Dead-server protection during streaming is handled separately: if
    // llama-server crashes mid-stream the reqwest byte-stream returns an
    // error, which forward_chat_completion surfaces as ForwardError::UpstreamDead
    // and the handler clears stale state for the next request.
    let client = Client::builder()
        .pool_max_idle_per_host(10)
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;

    // Background poller for llama.cpp's native `/slots` endpoint, feeding
    // the proxy dashboard's context-remaining display. It runs as its own
    // isolated Tokio task (see `slots_poller` module docs for the
    // backoff/lifecycle design) and is joined below after `axum::serve`
    // returns, so it never outlives the server or gets left detached.
    let slots_cache = Arc::new(SlotsCache::new());
    let slots_poller = spawn_slots_poller(
        Arc::clone(&runtime_port),
        client.clone(),
        Arc::clone(&slots_cache),
        cancel.clone(),
    );

    // Upstream-degradation watchdog, shared between the request path (strike
    // recording + recycle) and the dashboard (counter surfacing).
    let upstream_health = Arc::new(UpstreamHealth::new());

    // Shared cache state (constructed once, shared across all requests).
    // Always initialized; the `cache_enabled` guard prevents acquire() when disabled.
    let slot_gate = Arc::new(Semaphore::new(1));
    let clear_all_pending = Arc::new(AtomicBool::new(false));
    let per_session_cleared = Arc::new(DashSet::new());
    let server_start_time = Arc::new(AtomicU64::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    ));
    let last_loaded_session = Arc::new(tokio::sync::RwLock::new(None));

    // Background byte-budget eviction, so cached session slot files don't
    // accumulate without bound. Only runs when there's a slot_dir to sweep;
    // joined below on shutdown like the other background tasks.
    let lru_eviction = slot_dir.as_ref().map(|dir| {
        crate::slot_eviction::spawn_eviction_task(dir.clone(), disk_budget, cancel.clone())
    });

    let dashboard = Arc::new(DashboardState::new(
        Arc::new(ActiveConnectionsRegistry::new()),
        slots_cache,
        Arc::new(ContextMetricsStore::new()),
        Arc::clone(&upstream_health),
        Arc::new(CacheStatusCache::new()),
        Arc::new(CacheMetricsStore::new()),
        agent_metrics,
        Arc::clone(&runtime_port),
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
        dashboard,
        settings: Arc::new(SettingsCache::new(settings_repo)),
        shutdown: cancel.clone(),
        upstream_health,
        calibration: Arc::new(TokenCalibration::new()),
        inference_override,
        cache_enabled,
        slot_dir,
        slot_gate,
        clear_all_pending,
        per_session_cleared,
        server_start_time,
        last_loaded_session,
    };

    // Everything a client can reach with credentials. Grouped separately from
    // `/health` so `route_layer` can require the bearer token here without
    // closing the one endpoint a supervisor or a load balancer needs to poll
    // before it has any credentials to poll with.
    let mut protected = Router::new()
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/embeddings", post(crate::embeddings::embeddings))
        .route("/v1/proxy/status", get(handle_proxy_status))
        .route("/v1/proxy/status/stream", get(handle_proxy_status_stream))
        .route("/v1/proxy/cache/clear", post(handle_proxy_cache_clear))
        .route("/mcp", post(post_mcp).get(get_mcp).delete(delete_mcp));

    // Installed only when a token is configured, so the unauthenticated
    // loopback default — still the common case — pays nothing per request.
    if let Some(expected) = access.expected_authorization() {
        let expected: Arc<str> = Arc::from(expected);
        protected = protected.route_layer(axum::middleware::from_fn_with_state(
            expected,
            crate::access::bearer_guard,
        ));
    }

    let app = Router::new()
        .route("/health", get(health_check))
        .merge(protected)
        // Host allowlist: always on, and outside the router so it covers
        // `/health` and unmatched paths too. This is the DNS-rebinding guard;
        // see the `access` module for why CORS alone does not cover it.
        .layer(axum::middleware::from_fn_with_state(
            Arc::new(access.clone()),
            crate::access::host_guard,
        ))
        // LocalOnly CORS: mirrors the Axum web server's default security posture.
        // Only localhost, 127.0.0.1, ::1, and tauri://localhost origins are accepted.
        //
        // Outermost deliberately: it answers OPTIONS preflight itself, and a
        // preflight that reached the guards above would be refused for carrying
        // credentials it is not allowed to carry yet.
        .layer(build_cors_layer(&access.cors))
        .with_state(state);

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
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok"
    }))
}

/// Percentage shaved off a model's raw context window when advertised via
/// `/v1/models`.
///
/// Reserves headroom for the tool-schema JSON and chat-template tokens that a
/// client's own char→token budget estimate (e.g. the VS Code LLM Gateway's
/// `CHARS_PER_TOKEN = 4`) does not account for. Advertising slightly less than
/// the true ceiling makes such clients begin proactive context compaction
/// before the real limit is hit, avoiding upstream context-overflow rejections
/// on the final turns of a long session.
const CONTEXT_WINDOW_SAFETY_MARGIN_PCT: u64 = 8;

/// Apply [`CONTEXT_WINDOW_SAFETY_MARGIN_PCT`] to a raw context-window token
/// count, returning the value to advertise to clients.
fn advertised_context_window(raw_ctx: u64) -> u64 {
    raw_ctx.saturating_mul(100 - CONTEXT_WINDOW_SAFETY_MARGIN_PCT) / 100
}

/// List all models from the catalog in OpenAI format.
///
/// Every model advertises the context it would actually be served with —
/// clients like the GitHub Copilot LLM Gateway extension read this endpoint
/// ONCE when building their model picker (typically before any model is
/// running), so the pre-launch advertisement must already reflect the real
/// serving context or clients budget against a stale floor for the entire
/// session:
///
/// * **Non-running models**: `min(static GGUF context_length, default_ctx)`
///   — `default_ctx` is the same value `admit` will launch
///   the model with on its first request.
/// * **The currently running model**: its full live `effective_ctx` (the
///   real `--ctx-size` llama-server was launched with), which also drives
///   the per-request truncation budget in
///   [`crate::forward::forward_chat_completion`] — advertised and enforced
///   values stay in lockstep.
///
/// Both are shaved by [`CONTEXT_WINDOW_SAFETY_MARGIN_PCT`] before being
/// advertised, reserving headroom for tool-schema JSON and chat-template
/// tokens that a client's own char→token budget does not account for.
async fn list_models(State(state): State<AppState>) -> impl IntoResponse {
    debug!("GET /v1/models");

    match state.catalog_port.list_models().await {
        Ok(mut models) => {
            // Pinned mode refuses every other model, so advertising the rest
            // of the catalog would offer a BYOK client a choice that can only
            // come back as PinnedModelMismatch. Filtering the summaries here
            // rather than the finished response also keeps the variants below
            // correct for free — they are built from what survives.
            //
            // Profile variants of the pinned model stay: a profile changes
            // only the request body, never which model actually runs, so it
            // cannot trip the guard.
            if let Some(pinned) = state.runtime_port.pinned_model() {
                models.retain(|m| m.name == pinned);
            }

            let mut response = ModelsResponse::from_summaries(models, state.default_ctx);

            // Apply safety margin to every model's context_window.
            for model in &mut response.data {
                model.context_window = model.context_window.map(advertised_context_window);
            }

            if let Some(target) = state.runtime_port.current_model().await
                && let Some(model) = response.data.iter_mut().find(|m| m.id == target.model_name)
            {
                model.context_window = Some(advertised_context_window(target.effective_ctx));
            }

            // Append `{model}:{profile}` variants for profiles the user opted
            // into listing. Built from the base entries above, so they inherit
            // the context window each model would actually be served with.
            let variants = variant_entries(
                &response.data,
                state
                    .settings
                    .get()
                    .await
                    .inference_profiles
                    .as_deref()
                    .unwrap_or_default(),
            );
            response.data.extend(variants);

            Json(response).into_response()
        }
        Err(e) => {
            error!("Failed to list models: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal_error(&format!(
                    "Failed to list models: {e}"
                ))),
            )
                .into_response()
        }
    }
}

/// Return the unified proxy dashboard snapshot: active connections,
/// llama.cpp `/slots` state, and recent request metrics.
///
/// This is the shared data contract for the CLI TUI and web dashboard.
/// Fully replaces the old `{snapshots, total_requests}` shape — see the
/// `dashboard` module docs for why no backwards-compatible shim is kept.
async fn handle_proxy_status(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.dashboard.snapshot())
}

/// Subscribe to a live stream of [`crate::dashboard::DashboardSnapshot`]
/// updates via Server-Sent Events.
///
/// Uses hydrate-then-stream semantics (via [`gglib_sse::Broadcaster`]): the
/// client immediately receives one event carrying the current snapshot,
/// then a fresh snapshot on every subsequent publish tick — no waiting for
/// the next tick to see the current state.
async fn handle_proxy_status_stream(State(state): State<AppState>) -> impl IntoResponse {
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

/// Handle cache clear requests via `POST /v1/proxy/cache/clear`.
///
/// Two independent caches sit behind this endpoint:
///
/// * the **disk slot** layer, opt-in via `--cache`, cleared per-session with
///   `X-Gglib-Session-Id` or wholesale without it;
/// * llama-server's **host-RAM prompt cache** (`--cache-ram`), which has no
///   clear API of its own — recycling the process is the only way to drop it.
///
/// A global clear therefore also recycles the model. Without that, the common
/// configuration (RAM cache on, disk layer off) had no way to clear the only
/// cache it actually had: the endpoint reported `cache not enabled` and did
/// nothing, which is the least useful answer available.
///
/// A session-scoped clear is deliberately disk-only. Recycling the process to
/// service one session would discard every other session's cached prefix too.
async fn handle_proxy_cache_clear(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Extract optional session ID from header
    let session_id = headers
        .get("x-gglib-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    // Sanitize if provided — 400 on invalid input (safety-critical)
    if let Some(ref sid) = session_id
        && let Err(e) = crate::slots::sanitize_session_id(sid)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("invalid session id: {}", e)
            })),
        );
    }

    // ── Disk slot layer ───────────────────────────────────────────────────
    let disk = if state.cache_enabled {
        // base_url is unused by clear_cache; model_id 0 is a sentinel — it only
        // touches flags and hot-cache invalidation, not any specific model's slots.
        let Some(config) = state.build_stream_config(String::new(), 0) else {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "slot_dir not configured"
                })),
            );
        };
        match clear_cache(&config, session_id.as_deref()).await {
            Ok(()) => {
                if session_id.is_some() {
                    "session cleared"
                } else {
                    "all slots cleared"
                }
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                );
            }
        }
    } else {
        "disk cache not enabled"
    };

    // ── Host-RAM prompt cache ─────────────────────────────────────────────
    let ram = if session_id.is_some() {
        "RAM cache kept (session-scoped clear)"
    } else if state.dashboard.connections.is_empty() {
        // Gated on idle for the same reason as the watchdog recycle in
        // `chat_completions`: with `--parallel 1` an in-flight request owns the
        // only slot, and stop_current() would kill its live generation.
        info!("cache clear: recycling model to flush the host-RAM prompt cache");
        match state.runtime_port.stop_current().await {
            Ok(()) => "model recycled, RAM cache flushed",
            Err(e) => {
                warn!(error = %e, "cache clear: model recycle failed");
                "RAM cache not flushed (recycle failed)"
            }
        }
    } else {
        "RAM cache not flushed (request in flight; retry when idle)"
    };

    // Drop any frozen per-session calibration snapshot too, so a session
    // that explicitly cleared its cache re-baselines from the current
    // global ratio on its next request instead of reusing a pre-clear one.
    // Re-derive the sanitized (lowercased) form rather than reusing the raw
    // header value above — the validation call earlier only checks
    // `sanitize_session_id`'s `Err` case and discards its `Ok(String)`, so
    // `session_id` itself is still whatever case the client sent, and
    // `chat_completions` always keys snapshots by the lowercased form.
    if let Some(ref sid) = session_id {
        if let Ok(sanitized) = crate::slots::sanitize_session_id(sid) {
            state.calibration.clear_session(&sanitized);
        }
    } else {
        state.calibration.clear_all_sessions();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "message": format!("{disk}; {ram}"),
            "disk": disk,
            "ram": ram,
        })),
    )
}

/// Handle chat completions - ensure model is running and proxy to llama-server.
async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    debug!("POST /v1/chat/completions");

    // Canonicalize the system prompt and tool order once, up front, and
    // reuse the result for both the content-hash session id fallback below
    // and the forwarded request (forward_chat_completion no longer
    // re-canonicalizes) — avoids paying the parse/regex/serialize cost on
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
                    .body(axum::body::Body::from(format!("Invalid session ID: {}", e)))
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
        // this id now also keys `TokenCalibration`'s per-session budget
        // snapshot (see `forward_chat_completion`'s `calibration_session_id`),
        // which must work even when disk KV-slot caching is off. That's
        // exactly the case for hybrid/sliding-window-attention models, where
        // disk restore can't resume the prompt and is disabled by design (see
        // `slot_restore` in `gglib_runtime::llama::args`) — but the host-RAM
        // prompt cache this budget-stability fix protects still applies. The
        // actual disk save/restore activation stays independently gated on
        // `state.cache_enabled` at its own call site below, so widening where
        // this id is *derived* doesn't turn on disk caching when the feature
        // is off.
        crate::canonicalization::derive_fallback_session_id(&body)
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
    // base name, so a profile never causes a second model to be launched.
    let (model_name, request_profile) = match resolve_route(
        &model_name,
        configured_profiles,
        state.catalog_port.as_ref(),
    )
    .await
    {
        ModelRoute::Bare(model) => (model.to_owned(), None),
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

    // Watchdog: if the upstream tripped the consecutive-failure threshold on
    // prior requests (empty responses / first-byte timeouts while still
    // passing /health), recycle it now — before routing this request into a
    // server that has proven it is not producing output.
    //
    // Gate the recycle on the upstream being idle: this check runs before the
    // current request registers its connection, so a non-empty registry means
    // another request is in flight. With `--parallel 1` that request owns the
    // only slot, and stop_current() would kill its live generation. The `&&`
    // short-circuits so the recycle flag is left un-consumed when busy and is
    // honored by the next request that arrives while the upstream is idle.
    if state.dashboard.connections.is_empty() && state.upstream_health.take_recycle_request() {
        warn!("upstream watchdog: recycling degraded model before next request");
        let _ = state.runtime_port.stop_current().await;
    }

    // The one catalog round-trip this request pays for. Resolved here rather
    // than inside `forward_chat_completion` — same single lookup either way,
    // but doing it before the model is ensured running means a request the
    // loaded model could never serve can be refused without first paying for a
    // model swap to discover that. An unresolvable model yields a pass-through
    // context, leaving `admit` below to report it as it always
    // has.
    let model_context =
        gglib_core::request_pipeline::resolve(state.catalog_port.as_ref(), Some(&model_name)).await;

    // An embedding model cannot answer this. gglib launches models tagged
    // `embedding` with `--embeddings`, which llama-server reads as "restrict to
    // only the embedding use case" — that server refuses chat completions
    // outright. Forwarding anyway would evict whatever is currently serving
    // chat, load the embedding model, and collect a 501, leaving the endpoint
    // worse off than before the request arrived.
    //
    // An unresolvable model has an empty tag set here, so it falls through to
    // `admit` and its ModelNotFound exactly as before.
    if model_context
        .tags
        .iter()
        .any(|t| t == crate::embeddings::EMBEDDING_TAG)
    {
        info!(
            model = %model_name,
            "refusing chat completion for an embedding-only model"
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::embedding_model_cannot_chat(&model_name)),
        )
            .into_response();
    }

    // Join the admission queue.
    //
    // This is where a request for a model that is not loaded waits — batched
    // with every other request for the same model, so one swap serves all of
    // them rather than each paying for its own. A slow 200 beats a fast 503 for
    // an OpenAI-compatible client, which treats 503 as terminal (see the
    // UpstreamDead path below, which already avoids 503 for that reason).
    //
    // `admission.lease` is held for the whole of this request — moved into
    // `ForwardRequest` below — and is what stops the model being swapped out
    // from under a response that is still streaming.
    let admission = match state
        .runtime_port
        .admit(
            &model_name,
            num_ctx,
            state.default_ctx,
            gglib_core::ports::LaunchOverrides::default(),
        )
        .await
    {
        Ok(admission) => admission,
        Err(e) => {
            return handle_runtime_error(e);
        }
    };
    let target = admission.target.clone();
    let lease = admission.lease;

    // If the model was just restarted, invalidate all pending cache slots.
    //
    // A single fresh spawn can satisfy several requests that were queued
    // waiting on it, and each carries `just_started = true`. Dedup so exactly
    // one performs the invalidation: CAS the stored server-start time from the
    // value we observed to `now`. Only the first request wins the swap; the
    // rest see the already-updated value and skip (no repeated WARN, no
    // redundant re-invalidation). The stored start time doubles as the mtime
    // guard's cutoff, so the winning swap sets it in the same step.
    if target.just_started {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let prev = state.server_start_time.load(AtomicOrdering::SeqCst);
        if now > prev
            && state
                .server_start_time
                .compare_exchange(prev, now, AtomicOrdering::SeqCst, AtomicOrdering::SeqCst)
                .is_ok()
        {
            tracing::warn!("Llama-server restart detected — invalidating KV cache slots");
            state.clear_all_pending.store(true, AtomicOrdering::SeqCst);
            // Invalidate hot cache — the server state is fresh, nothing is loaded.
            *state.last_loaded_session.write().await = None;
        }
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

    // Register this request in the active-connections dashboard registry.
    // The returned guard unregisters on drop (see `connections` module docs)
    // — normal completion, early return, client disconnect, or panic all
    // clean up without any explicit unregister call at each exit point.
    //
    // The admission lease rides along on the guard (see `connections` module
    // docs): it must outlive the response, including across the streaming
    // path's spawned task, and the guard already goes exactly that far.
    let connection = state
        .dashboard
        .connections
        .register(model_name.clone(), is_streaming, Some(target.effective_ctx))
        .holding(lease);

    // Global defaults come from the same snapshot the profile list did.
    let sampling = SamplingLayers {
        cli_override: state.inference_override.clone(),
        profile: request_profile.clone(),
        global: settings.inference_defaults.clone(),
        trust_client_sampling: settings.trust_client_sampling.unwrap_or(false),
    };

    // Clone body before forwarding — Bytes is reference-counted so this is
    // O(1).  Needed to retry with the original payload if the upstream dies.
    let body_for_retry = body.clone();

    // Build StreamConfig for this request (Some only when cache is enabled).
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
        client: &state.client,
        upstream_url: &upstream_url,
        headers: &headers,
        body,
        is_streaming,
        model_name: &model_name,
        effective_ctx: target.effective_ctx,
        context: model_context.clone(),
        metrics: state.dashboard.metrics.clone(),
        sampling,
        connection,
        upstream_health: state.upstream_health.clone(),
        calibration: state.calibration.clone(),
        calibration_session_id: sanitized_session_id.as_deref(),
        cache_metrics: state.dashboard.cache_metrics.clone(),
    };

    // Forward the request, optionally wrapped in cache lifecycle. `Some(cfg)`
    // in `stream_config` already implies `state.cache_enabled` (see its
    // construction just above), so matching on `(session_id, stream_config)`
    // alone — without a redundant outer `cache_enabled` check — covers every
    // case: cache disabled, cache enabled but no session id/config, and
    // cache enabled with both all fall into the same "no triple" arm below.
    let response = match (&sanitized_session_id, &stream_config) {
        (Some(sid), Some(cfg)) => {
            if !is_streaming {
                // Non-streaming with cache: wrap in run_with_cache (fail-open internally)
                let (resp, _restore_result) =
                    run_with_cache(cfg, &state.slot_gate, sid, || req.send(None, None, None))
                        .await
                        .expect(
                        "run_with_cache only returns Err on sanitization failure, which is already checked",
                    );
                resp
            } else {
                // Streaming with cache: use prepare_streaming_cycle + sse_stream::spawn_and_return
                let (permit, cfg, sid) =
                    resolve_cache_triple(cfg, state.slot_gate.clone(), sid).await;
                req.send(permit, cfg, sid).await
            }
        }
        // Cache disabled, or cache enabled but no session id/config: direct call
        _ => req.send(None, None, None).await,
    };

    // Handle UpstreamDead from the primary forward (only possible when cache is disabled
    // or no session ID — cache-wrapped paths return Ok(Response) internally)
    match response {
        Ok(resp) => resp,
        Err(ForwardError::UpstreamDead) => {
            // llama-server was dead after admission returned a stale port.
            // Strategy:
            //   1. Clear stale state via stop_current().
            //   2. Re-admit — the queue does the waiting now, so one request
            //      drives the restart and concurrent requests are batched
            //      behind it rather than surfacing a 503 to the client (the VS
            //      Code LLM Gateway treats 503 as a terminal error).
            //   3. Retry the forward once with the cloned body.
            warn!(
                upstream = %upstream_url,
                "upstream dead — clearing stale state and restarting model for transparent retry"
            );
            let _ = state.runtime_port.stop_current().await;

            // AdmissionTimeout is deliberately not retried here: it means the
            // GPU is oversubscribed rather than that this model is still
            // loading, so it falls through to a 503 + Retry-After and the
            // client controls its own backoff. (PR #587)
            let retry_admission = match state
                .runtime_port
                .admit(
                    &model_name,
                    num_ctx,
                    state.default_ctx,
                    gglib_core::ports::LaunchOverrides::default(),
                )
                .await
            {
                Ok(admission) => admission,
                Err(e) => return handle_runtime_error(e),
            };
            let new_target = retry_admission.target.clone();
            let retry_lease = retry_admission.lease;

            let retry_url = format!("{}/v1/chat/completions", new_target.base_url);
            // Re-read settings for the retry: the model was just relaunched,
            // so this is a fresh point in time. The profile is deliberately
            // not re-resolved — the client asked for a specific one.
            let retry_settings = state.settings.get().await;
            let retry_sampling = SamplingLayers {
                cli_override: state.inference_override.clone(),
                profile: request_profile.clone(),
                global: retry_settings.inference_defaults.clone(),
                trust_client_sampling: retry_settings.trust_client_sampling.unwrap_or(false),
            };

            // Fresh connection for the retried attempt — the original guard
            // (moved into the first `forward_chat_completion` call above)
            // was already dropped when that call returned `UpstreamDead`,
            // taking the first attempt's admission lease with it.
            let retry_connection = state
                .dashboard
                .connections
                .register(
                    model_name.clone(),
                    is_streaming,
                    Some(new_target.effective_ctx),
                )
                .holding(retry_lease);

            // Compute cache-aware permit/config/session_id for the retry.
            // Mirrors the normal-path pattern: acquire permit via
            // prepare_streaming_cycle, fail-open on error. Deliberately does
            // NOT branch on `is_streaming` the way the primary attempt does
            // above — a non-streaming retry still resolves the triple this
            // way rather than going through `run_with_cache`, matching this
            // path's existing behavior.
            // The disk-layer gate also applies here: the retry targets a freshly
            // spawned instance of the same model, so a partial-KV model stays on
            // the RAM-cache-only path (see the initial attempt above).
            let (retry_permit, retry_cfg, retry_session) = match (
                state.cache_enabled && new_target.slot_restore_supported,
                sanitized_session_id.as_ref(),
                state.build_stream_config(new_target.base_url.clone(), new_target.model_id),
            ) {
                (true, Some(sid), Some(cfg)) => {
                    resolve_cache_triple(&cfg, state.slot_gate.clone(), sid).await
                }
                _ => (None, None, None),
            };

            let retry_req = ForwardRequest {
                client: &state.client,
                upstream_url: &retry_url,
                headers: &headers,
                body: body_for_retry,
                is_streaming,
                model_name: &model_name,
                effective_ctx: new_target.effective_ctx,
                // The same context the first attempt used. A retry follows a
                // restart of the same model, so re-reading the catalog could
                // only return what is already in hand.
                context: model_context.clone(),
                metrics: state.dashboard.metrics.clone(),
                sampling: retry_sampling,
                connection: retry_connection,
                upstream_health: state.upstream_health.clone(),
                calibration: state.calibration.clone(),
                calibration_session_id: sanitized_session_id.as_deref(),
                cache_metrics: state.dashboard.cache_metrics.clone(),
            };

            match retry_req.send(retry_permit, retry_cfg, retry_session).await {
                Ok(resp) => resp,
                Err(_) => {
                    // Server failed immediately after a fresh restart —
                    // genuinely pathological; give up.
                    let mut resp = (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(ErrorResponse::model_loading()),
                    )
                        .into_response();
                    if let Ok(value) = "5".parse() {
                        resp.headers_mut().insert("retry-after", value);
                    }
                    resp
                }
            }
        }
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

/// Convert ModelRuntimeError to HTTP response with appropriate status code.
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
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check() {
        let response = health_check().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_admission_timeout_returns_503_with_retry_after() {
        let err = ModelRuntimeError::AdmissionTimeout("test timeout".to_string());
        let response = handle_runtime_error(err);
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            response
                .headers()
                .contains_key(axum::http::header::RETRY_AFTER)
        );
    }

    /// The advertised hint must come from the shared policy, not a literal, so
    /// it cannot drift from the backoff our own clients actually apply.
    #[tokio::test]
    async fn retry_after_is_a_delay_the_policy_could_produce() {
        let response = handle_runtime_error(ModelRuntimeError::AdmissionTimeout("c".to_string()));
        let policy = RetryPolicy::default();

        let advertised = response
            .headers()
            .get(axum::http::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .expect("Retry-After must be a whole number of seconds");

        assert!(advertised >= 1, "a zero hint invites an immediate hot loop");
        assert!(
            advertised <= policy.max_backoff.as_secs(),
            "advertising longer than the policy's own ceiling asks clients to \
             wait longer than we ever would: {advertised}s"
        );
        assert!(
            std::time::Duration::from_secs(advertised) < policy.total_deadline,
            "a hint at or past the whole budget leaves no room for a retry"
        );
    }

    /// An oversubscribed queue and ordinary loading are indistinguishable on
    /// the wire — both serialise to `service_unavailable` — so the reason
    /// header is the only way a dashboard can tell them apart.
    #[tokio::test]
    async fn only_an_admission_timeout_carries_the_retry_reason_header() {
        let queued_out = handle_runtime_error(ModelRuntimeError::AdmissionTimeout("c".to_string()));
        assert_eq!(
            queued_out
                .headers()
                .get(RETRY_REASON_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some(RETRY_REASON_ADMISSION)
        );

        let loading = handle_runtime_error(ModelRuntimeError::ModelLoading);
        assert_eq!(loading.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            loading
                .headers()
                .contains_key(axum::http::header::RETRY_AFTER),
            "loading is still retryable and still advertises a hint"
        );
        assert!(
            !loading.headers().contains_key(RETRY_REASON_HEADER),
            "only an admission timeout is labelled"
        );
    }

    /// A terminal error must not invite a retry.
    #[tokio::test]
    async fn non_retryable_errors_carry_neither_header() {
        let response = handle_runtime_error(ModelRuntimeError::ModelNotFound("nope".to_string()));
        assert_ne!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            !response
                .headers()
                .contains_key(axum::http::header::RETRY_AFTER)
        );
        assert!(!response.headers().contains_key(RETRY_REASON_HEADER));
    }
}
