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
use tower_http::cors::{Any, CorsLayer};
use tracing::{debug, error, info, warn};

use gglib_core::ports::{
    ModelCatalogPort, ModelRuntimeError, ModelRuntimePort, SettingsRepository,
};
use gglib_mcp::McpService;

use crate::cache_lifecycle::{StreamConfig, clear_cache, run_with_cache};
use crate::connections::ActiveConnectionsRegistry;
use crate::council_proxy::{CouncilDeps, VIRTUAL_MODELS, handle_virtual_model, virtual_model_info};
use crate::dashboard::{CacheStatus, CacheStatusCache, DashboardState, spawn_dashboard_publisher};
use crate::forward::{ForwardError, forward_chat_completion};
use crate::mcp::handlers::{delete_mcp, get_mcp, post_mcp};
use crate::mcp::session::SessionManager;
use crate::metrics::ContextMetricsStore;
use crate::models::{ChatRoutingEnvelope, ErrorResponse, ModelInfo, ModelsResponse};
use crate::slots_poller::{SlotsCache, spawn_slots_poller};
use crate::token_calibration::TokenCalibration;
use crate::upstream_health::UpstreamHealth;
use dashmap::DashSet;
use gglib_sse::SseOptions;

/// Shared application state for the proxy server.
#[derive(Clone)]
pub(crate) struct AppState {
    /// HTTP client for forwarding requests to llama-server.
    client: Client,
    /// Port for managing model runtime.
    runtime_port: Arc<dyn ModelRuntimePort>,
    /// Port for listing and resolving models.
    catalog_port: Arc<dyn ModelCatalogPort>,
    /// MCP service for tool gateway.
    pub(crate) mcp: Arc<McpService>,
    /// Session manager for MCP Streamable HTTP sessions.
    pub(crate) sessions: SessionManager,
    /// Default context size when not specified in request.
    default_ctx: u64,
    /// Orchestrator services for virtual model routing.
    council: CouncilDeps,
    /// Unified proxy dashboard state: active-connections registry, llama.cpp
    /// `/slots` cache, and request metrics, plus the SSE broadcaster that
    /// pushes snapshots to `GET /v1/proxy/status/stream`. Replaces what were
    /// previously three separate `AppState` fields (`metrics`, `connections`,
    /// `slots`) — see `dashboard` module docs for the consolidation rationale.
    pub(crate) dashboard: Arc<DashboardState>,
    /// Settings repository for loading global inference defaults per-request.
    settings_repo: Arc<dyn SettingsRepository>,
    /// Consecutive-failure watchdog: trips a proactive model recycle when the
    /// upstream degrades to empty responses / first-byte timeouts while still
    /// passing its `/health` check.
    upstream_health: Arc<UpstreamHealth>,
    /// Per-model chars-per-token calibration, learned from upstream usage
    /// frames and used to size the truncation budget.
    calibration: Arc<TokenCalibration>,
    /// Whether KV cache persistence is enabled (opt-in via --cache).
    cache_enabled: bool,
    /// Resolved slot directory path (Some only when cache_enabled).
    slot_dir: Option<PathBuf>,
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
/// * `council` - Orchestrator services for virtual model routing
/// * `cancel` - Cancellation token for graceful shutdown
/// * `settings_repo` - Settings repository for loading global inference defaults
/// * `disk_budget` - Byte budget for the on-disk slot cache eviction sweep.
///   Only consulted when `slot_dir` is `Some`.
///
/// # Returns
///
/// Returns `Ok(())` on clean shutdown, or an error if the server fails.
#[allow(clippy::too_many_arguments)]
pub async fn serve(
    listener: TcpListener,
    default_ctx: u64,
    runtime_port: Arc<dyn ModelRuntimePort>,
    catalog_port: Arc<dyn ModelCatalogPort>,
    mcp: Arc<McpService>,
    council: CouncilDeps,
    cancel: CancellationToken,
    settings_repo: Arc<dyn SettingsRepository>,
    cache_enabled: bool,
    slot_dir: Option<PathBuf>,
    disk_budget: crate::slot_eviction::DiskBudget,
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
        Arc::new(crate::cache_metrics::CacheMetricsStore::new()),
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
        council,
        dashboard,
        settings_repo,
        upstream_health,
        calibration: Arc::new(TokenCalibration::new()),
        cache_enabled,
        slot_dir,
        slot_gate,
        clear_all_pending,
        per_session_cleared,
        server_start_time,
        last_loaded_session,
    };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/proxy/status", get(handle_proxy_status))
        .route("/v1/proxy/status/stream", get(handle_proxy_status_stream))
        .route("/v1/proxy/cache/clear", post(handle_proxy_cache_clear))
        .route("/mcp", post(post_mcp).get(get_mcp).delete(delete_mcp))
        // Permissive CORS: this proxy only ever binds to 127.0.0.1 for local
        // developer use (CLI or the Tauri GUI) and strips `Authorization`
        // before forwarding upstream (see `forward.rs`), so there's no
        // credentialed session to protect and no benefit to restricting
        // origins — same rationale as `CorsConfig::AllowAll` in gglib-axum.
        // Without this, browser-based clients (notably the Tauri webview,
        // which runs on the `tauri://localhost` / `http://tauri.localhost`
        // origin) are blocked from calling this proxy's endpoints, including
        // the `EventSource` connection to `/v1/proxy/status/stream`.
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
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
/// Appends the three virtual council model entries after the catalog models.
///
/// Every model advertises the context it would actually be served with —
/// clients like the GitHub Copilot LLM Gateway extension read this endpoint
/// ONCE when building their model picker (typically before any model is
/// running), so the pre-launch advertisement must already reflect the real
/// serving context or clients budget against a stale floor for the entire
/// session:
///
/// * **Non-running models**: `min(static GGUF context_length, default_ctx)`
///   — `default_ctx` is the same value `ensure_model_running` will launch
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
        Ok(models) => {
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

            // Append virtual council models.
            let virtuals: Vec<ModelInfo> = vec![
                virtual_model_info(
                    "gglib-council",
                    "Auto mode — runs the full Director/Worker pipeline with no approval gates.",
                ),
                virtual_model_info(
                    "gglib-council:interactive",
                    "Interactive mode — pauses at the plan gate; resume by replying 'yes'.",
                ),
                virtual_model_info(
                    "gglib-council:native",
                    "Native mode — use POST /api/council/run for the full API.",
                ),
            ];
            response.data.extend(virtuals);
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
    Arc::clone(&state.dashboard.broadcaster)
        .subscribe_with_hydration(current, SseOptions::default())
}

/// Handle cache clear requests via `POST /v1/proxy/cache/clear`.
///
/// Optionally accepts `X-Gglib-Session-Id` header to clear a single session;
/// without it, all slot files are cleared. Returns 200 OK with status JSON.
async fn handle_proxy_cache_clear(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Cache disabled → skip (idempotent no-op)
    if !state.cache_enabled {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "skipped",
                "message": "cache not enabled"
            })),
        );
    }

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

    let slot_dir = match &state.slot_dir {
        Some(d) => d.clone(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "slot_dir not configured"
                })),
            );
        }
    };

    let config = StreamConfig {
        client: state.client.clone(),
        base_url: String::new(), // Not used by clear_cache
        slot_dir,
        model_id: 0, // Sentinel — clear_cache only uses flags and hot-cache invalidation
        clear_all_pending: state.clear_all_pending.clone(),
        per_session_cleared: state.per_session_cleared.clone(),
        server_start_time: state.server_start_time.clone(),
        last_loaded_session: state.last_loaded_session.clone(),
    };

    match clear_cache(&config, session_id.as_deref()).await {
        Ok(()) => {
            let msg = if session_id.is_some() {
                "session cleared"
            } else {
                "all slots cleared"
            };
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "ok",
                    "message": msg
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": e.to_string()
            })),
        ),
    }
}

/// Handle chat completions - ensure model is running and proxy to llama-server.
async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    debug!("POST /v1/chat/completions");

    // Canonicalize once, up front, and reuse the result for both the
    // content-hash session id fallback below and the forwarded request
    // (forward_chat_completion no longer re-canonicalizes) — avoids paying
    // the parse/regex/serialize cost on this ~150KB+ body twice per request.
    let body = crate::canonicalization::canonicalize_system_prompt(body);

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
    } else if state.cache_enabled {
        // No explicit header — most clients (VS Code Copilot's LLM Gateway
        // extension, curl, anything else speaking plain OpenAI-compatible
        // chat completions) have no idea X-Gglib-Session-Id exists. Derive a
        // stable fallback from the request content itself so the cache
        // still works without any client cooperation.
        crate::canonicalization::derive_fallback_session_id(&body)
    } else {
        None
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

    // Intercept virtual council model names before forwarding.
    if VIRTUAL_MODELS.contains(&model_name.as_str()) {
        return handle_virtual_model(
            &state.council,
            &state.dashboard.connections,
            &model_name,
            &body,
        )
        .await;
    }

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

    // Ensure the model is running with specified context or default
    let target = match state
        .runtime_port
        .ensure_model_running(&model_name, num_ctx, state.default_ctx)
        .await
    {
        Ok(target) => target,
        Err(e) => {
            return handle_runtime_error(e);
        }
    };

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

    // Register this request in the active-connections dashboard registry.
    // The returned guard unregisters on drop (see `connections` module docs)
    // — normal completion, early return, client disconnect, or panic all
    // clean up without any explicit unregister call at each exit point.
    let connection = state.dashboard.connections.register(
        model_name.clone(),
        is_streaming,
        Some(target.effective_ctx),
    );

    // Load global inference defaults for this request.
    let global_inference_defaults = state
        .settings_repo
        .load()
        .await
        .ok()
        .and_then(|s| s.inference_defaults);

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
        state.slot_dir.as_ref().map(|dir| StreamConfig {
            client: state.client.clone(),
            base_url: target.base_url.clone(),
            slot_dir: dir.clone(),
            model_id: target.model_id,
            clear_all_pending: state.clear_all_pending.clone(),
            per_session_cleared: state.per_session_cleared.clone(),
            server_start_time: state.server_start_time.clone(),
            last_loaded_session: state.last_loaded_session.clone(),
        })
    } else {
        None
    };

    // Forward the request, optionally wrapped in cache lifecycle
    let response = if state.cache_enabled {
        if let (Some(sid), Some(cfg)) = (&sanitized_session_id, &stream_config) {
            if !is_streaming {
                // Non-streaming with cache: wrap in run_with_cache (fail-open internally)
                let (resp, _restore_result) = run_with_cache(cfg, &state.slot_gate, sid, || async {
                    forward_chat_completion(
                        &state.client,
                        &upstream_url,
                        &headers,
                        body,
                        is_streaming,
                        &model_name,
                        target.effective_ctx,
                        state.catalog_port.clone(),
                        state.dashboard.metrics.clone(),
                        global_inference_defaults,
                        connection,
                        state.upstream_health.clone(),
                        state.calibration.clone(),
                        state.dashboard.cache_metrics.clone(),
                        None,
                        None,
                        None,
                    )
                    .await
                })
                .await
                .expect(
                    "run_with_cache only returns Err on sanitization failure, which is already checked",
                );
                resp
            } else {
                // Streaming with cache: use prepare_streaming_cycle + sse_stream::spawn_and_return
                let sid = sid.clone();
                let cfg = cfg.clone();
                match crate::cache_lifecycle::prepare_streaming_cycle(
                    &cfg,
                    state.slot_gate.clone(),
                    &sid,
                )
                .await
                {
                    Ok((permit, _sanitized, _restore_result)) => {
                        forward_chat_completion(
                            &state.client,
                            &upstream_url,
                            &headers,
                            body,
                            is_streaming,
                            &model_name,
                            target.effective_ctx,
                            state.catalog_port.clone(),
                            state.dashboard.metrics.clone(),
                            global_inference_defaults,
                            connection,
                            state.upstream_health.clone(),
                            state.calibration.clone(),
                            state.dashboard.cache_metrics.clone(),
                            Some(permit),
                            Some(cfg),
                            Some(sid),
                        )
                        .await
                    }
                    Err(_) => {
                        // Fail-open: proceed without cache for streaming too
                        forward_chat_completion(
                            &state.client,
                            &upstream_url,
                            &headers,
                            body,
                            is_streaming,
                            &model_name,
                            target.effective_ctx,
                            state.catalog_port.clone(),
                            state.dashboard.metrics.clone(),
                            global_inference_defaults,
                            connection,
                            state.upstream_health.clone(),
                            state.calibration.clone(),
                            state.dashboard.cache_metrics.clone(),
                            None,
                            None,
                            None,
                        )
                        .await
                    }
                }
            }
        } else {
            // Cache enabled but no session ID or config: direct call
            forward_chat_completion(
                &state.client,
                &upstream_url,
                &headers,
                body,
                is_streaming,
                &model_name,
                target.effective_ctx,
                state.catalog_port.clone(),
                state.dashboard.metrics.clone(),
                global_inference_defaults,
                connection,
                state.upstream_health.clone(),
                state.calibration.clone(),
                state.dashboard.cache_metrics.clone(),
                None,
                None,
                None,
            )
            .await
        }
    } else {
        // Cache disabled: direct call
        forward_chat_completion(
            &state.client,
            &upstream_url,
            &headers,
            body,
            is_streaming,
            &model_name,
            target.effective_ctx,
            state.catalog_port.clone(),
            state.dashboard.metrics.clone(),
            global_inference_defaults,
            connection,
            state.upstream_health.clone(),
            state.calibration.clone(),
            state.dashboard.cache_metrics.clone(),
            None,
            None,
            None,
        )
        .await
    };

    // Handle UpstreamDead from the primary forward (only possible when cache is disabled
    // or no session ID — cache-wrapped paths return Ok(Response) internally)
    match response {
        Ok(resp) => resp,
        Err(ForwardError::UpstreamDead) => {
            // llama-server was dead after ensure_model_running() returned a
            // stale port.  Strategy:
            //   1. Clear stale state via stop_current().
            //   2. Poll ensure_model_running() until it returns Ok (one
            //      request drives the restart; concurrent requests wait here
            //      rather than surfacing a 503 to the client, because the VS
            //      Code LLM Gateway treats 503 as a terminal error).
            //   3. Retry the forward once with the cloned body.
            warn!(
                upstream = %upstream_url,
                "upstream dead — clearing stale state and restarting model for transparent retry"
            );
            let _ = state.runtime_port.stop_current().await;

            // Bounded polling: give up after 130 s (120 s health-check window
            // plus 10 s of margin).
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(130);

            let new_target = loop {
                match state
                    .runtime_port
                    .ensure_model_running(&model_name, num_ctx, state.default_ctx)
                    .await
                {
                    Ok(t) => break t,
                    Err(ModelRuntimeError::ModelLoading) => {
                        // NOTE: ContentionTimeout is intentionally NOT retried here — it is a
                        // resource contention signal (not transient loading). It falls through to
                        // Err(e) below, returning 503 + Retry-After so the client controls backoff.
                        // (PR #587)
                        if std::time::Instant::now() >= deadline {
                            warn!("Timed out waiting for model restart after upstream failure");
                            return handle_runtime_error(ModelRuntimeError::ModelLoading);
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                    Err(e) => return handle_runtime_error(e),
                }
            };

            let retry_url = format!("{}/v1/chat/completions", new_target.base_url);
            let retry_defaults = state
                .settings_repo
                .load()
                .await
                .ok()
                .and_then(|s| s.inference_defaults);

            // Fresh connection for the retried attempt — the original guard
            // (moved into the first `forward_chat_completion` call above)
            // was already dropped when that call returned `UpstreamDead`.
            let retry_connection = state.dashboard.connections.register(
                model_name.clone(),
                is_streaming,
                Some(new_target.effective_ctx),
            );

            // Compute cache-aware permit/config/session_id for the retry.
            // Mirrors the normal-path pattern: acquire permit via
            // prepare_streaming_cycle, fail-open on error.
            // Same disk-layer gate as the initial attempt — the retry targets a
            // freshly spawned instance of the same model, so a partial-KV
            // model stays on the RAM-cache-only path here too.
            let (retry_permit, retry_cfg, retry_session) =
                if state.cache_enabled && new_target.slot_restore_supported {
                    if let (Some(sid), Some(slot_dir)) = (&sanitized_session_id, &state.slot_dir) {
                        let cfg = StreamConfig {
                            client: state.client.clone(),
                            base_url: new_target.base_url.clone(),
                            slot_dir: slot_dir.clone(),
                            model_id: new_target.model_id,
                            clear_all_pending: state.clear_all_pending.clone(),
                            per_session_cleared: state.per_session_cleared.clone(),
                            server_start_time: state.server_start_time.clone(),
                            last_loaded_session: state.last_loaded_session.clone(),
                        };

                        match crate::cache_lifecycle::prepare_streaming_cycle(
                            &cfg,
                            state.slot_gate.clone(),
                            sid,
                        )
                        .await
                        {
                            Ok((permit, _sanitized, _restore)) => {
                                (Some(permit), Some(cfg), Some(sid.clone()))
                            }
                            Err(_) => (None, None, None), // fail-open
                        }
                    } else {
                        (None, None, None)
                    }
                } else {
                    (None, None, None)
                };

            match forward_chat_completion(
                &state.client,
                &retry_url,
                &headers,
                body_for_retry,
                is_streaming,
                &model_name,
                new_target.effective_ctx,
                state.catalog_port.clone(),
                state.dashboard.metrics.clone(),
                retry_defaults,
                retry_connection,
                state.upstream_health.clone(),
                state.calibration.clone(),
                state.dashboard.cache_metrics.clone(),
                retry_permit,
                retry_cfg,
                retry_session,
            )
            .await
            {
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

/// Convert ModelRuntimeError to HTTP response with appropriate status code.
fn handle_runtime_error(err: ModelRuntimeError) -> Response {
    let status = StatusCode::from_u16(err.suggested_status_code())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let error_response = ErrorResponse::from(err);

    let mut response = (status, Json(error_response)).into_response();

    // Add Retry-After header for retryable errors (503)
    if status == StatusCode::SERVICE_UNAVAILABLE
        && let Ok(value) = "5".parse()
    {
        response.headers_mut().insert("retry-after", value);
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
    async fn test_contention_timeout_returns_503_with_retry_after() {
        let err = ModelRuntimeError::ContentionTimeout("test contention".to_string());
        let response = handle_runtime_error(err);
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            response
                .headers()
                .contains_key(axum::http::header::RETRY_AFTER)
        );
    }
}
