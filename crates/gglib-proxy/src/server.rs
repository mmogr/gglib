//! Axum HTTP server for the OpenAI-compatible proxy.
//!
//! This module provides the `serve()` function that runs the proxy server
//! using a pre-bound TcpListener (from the supervisor).

use std::sync::Arc;

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
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use gglib_core::ports::{
    ModelCatalogPort, ModelRuntimeError, ModelRuntimePort, SettingsRepository,
};
use gglib_mcp::McpService;

use crate::connections::ActiveConnectionsRegistry;
use crate::council_proxy::{CouncilDeps, VIRTUAL_MODELS, handle_virtual_model, virtual_model_info};
use crate::forward::{ForwardError, forward_chat_completion};
use crate::mcp::handlers::{delete_mcp, get_mcp, post_mcp};
use crate::mcp::session::SessionManager;
use crate::metrics::ContextMetricsStore;
use crate::models::{ChatRoutingEnvelope, ErrorResponse, ModelInfo, ModelsResponse};
use crate::slots_poller::{SlotsCache, spawn_slots_poller};

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
    /// Ring-buffer store of per-request context metrics, exposed via
    /// `GET /v1/proxy/status`.
    pub(crate) metrics: Arc<ContextMetricsStore>,
    /// Registry of in-flight `/v1/chat/completions` connections, exposed via
    /// the future proxy dashboard endpoint.
    pub(crate) connections: Arc<ActiveConnectionsRegistry>,
    /// Cache of the most recent llama.cpp `/slots` poll, refreshed by the
    /// background poller spawned in `serve()`. Exposed via the future proxy
    /// dashboard endpoint.
    ///
    /// Not read anywhere yet — the dashboard endpoint that will consume it
    /// lands in a later phase. Wired into `AppState` now (rather than left
    /// as a bare local in `serve()`) so the poller's cache is available to
    /// handlers from day one with no further plumbing required.
    #[allow(dead_code)]
    pub(crate) slots: Arc<SlotsCache>,
    /// Settings repository for loading global inference defaults per-request.
    settings_repo: Arc<dyn SettingsRepository>,
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
    // the future proxy dashboard's context-remaining display. It runs as
    // its own isolated Tokio task (see `slots_poller` module docs for the
    // backoff/lifecycle design) and is joined below after `axum::serve`
    // returns, so it never outlives the server or gets left detached.
    let slots_cache = Arc::new(SlotsCache::new());
    let slots_poller = spawn_slots_poller(
        Arc::clone(&runtime_port),
        client.clone(),
        Arc::clone(&slots_cache),
        cancel.clone(),
    );

    let state = AppState {
        client,
        runtime_port,
        catalog_port,
        mcp,
        sessions: SessionManager::new(),
        default_ctx,
        council,
        metrics: Arc::new(ContextMetricsStore::new()),
        connections: Arc::new(ActiveConnectionsRegistry::new()),
        slots: slots_cache,
        settings_repo,
    };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/proxy/status", get(handle_proxy_status))
        .route("/mcp", post(post_mcp).get(get_mcp).delete(delete_mcp))
        .with_state(state);

    info!("Proxy listening on {addr}");
    info!("Configure OpenWebUI to use: http://{addr}/v1");
    info!("MCP Streamable HTTP endpoint: http://{addr}/mcp");

    axum::serve(listener, app)
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await?;

    // Ensure the poller task is fully joined (not just cancelled-and-
    // detached) before `serve()` returns, so callers can rely on a clean
    // shutdown leaving no dangling tasks behind.
    if let Err(e) = slots_poller.await {
        warn!("proxy dashboard: /slots poller task panicked during shutdown: {e}");
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

/// List all models from the catalog in OpenAI format.
///
/// Appends the three virtual council model entries after the catalog models.
async fn list_models(State(state): State<AppState>) -> impl IntoResponse {
    debug!("GET /v1/models");

    match state.catalog_port.list_models().await {
        Ok(models) => {
            let mut response = ModelsResponse::from_summaries(models);
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

/// Return a snapshot of recent proxy request metrics.
///
/// Responds with the last 20 request snapshots and the total request count
/// since the proxy started.  This is the shared data contract for the CLI
/// TUI and web dashboard.
async fn handle_proxy_status(State(state): State<AppState>) -> impl IntoResponse {
    let snapshots = state.metrics.recent(20);
    let total_requests = state.metrics.total_requests();
    Json(serde_json::json!({
        "snapshots": snapshots,
        "total_requests": total_requests,
    }))
}

/// Handle chat completions - ensure model is running and proxy to llama-server.
async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    debug!("POST /v1/chat/completions");

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
        return handle_virtual_model(&state.council, &state.connections, &model_name, &body).await;
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

    // Build upstream URL
    let upstream_url = format!("{}/v1/chat/completions", target.base_url);
    debug!(
        upstream = %upstream_url,
        model_id = %target.model_id,
        model_name = %target.model_name,
        "Routing to llama-server"
    );

    // Register this request in the active-connections dashboard registry.
    // The returned guard unregisters on drop (see `connections` module docs)
    // — normal completion, early return, client disconnect, or panic all
    // clean up without any explicit unregister call at each exit point.
    let connection =
        state
            .connections
            .register(model_name.clone(), is_streaming, Some(target.effective_ctx));

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

    // Forward the request
    match forward_chat_completion(
        &state.client,
        &upstream_url,
        &headers,
        body,
        is_streaming,
        &model_name,
        state.catalog_port.clone(),
        state.metrics.clone(),
        global_inference_defaults,
        connection,
    )
    .await
    {
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
                        // Another request is already driving the restart.
                        // Sleep briefly then re-poll rather than returning a
                        // fatal 503 to the client.
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
            let retry_connection = state.connections.register(
                model_name.clone(),
                is_streaming,
                Some(new_target.effective_ctx),
            );

            match forward_chat_completion(
                &state.client,
                &retry_url,
                &headers,
                body_for_retry,
                is_streaming,
                &model_name,
                state.catalog_port.clone(),
                state.metrics.clone(),
                retry_defaults,
                retry_connection,
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
    let (status, error_response) = match &err {
        ModelRuntimeError::ModelLoading => {
            // 503 with Retry-After header
            (StatusCode::SERVICE_UNAVAILABLE, ErrorResponse::from(err))
        }
        ModelRuntimeError::ModelNotFound(_) => (StatusCode::NOT_FOUND, ErrorResponse::from(err)),
        ModelRuntimeError::ModelFileNotFound(_) => {
            (StatusCode::NOT_FOUND, ErrorResponse::from(err))
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, ErrorResponse::from(err)),
    };

    let mut response = (status, Json(error_response)).into_response();

    // Add Retry-After header for loading state
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
}
