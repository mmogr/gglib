//! HTTP handler implementation for the OpenAI-compatible proxy.

use super::models::*;
use crate::services::core::ModelService;
use crate::services::process_manager::ProcessManager;
use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bytes::Bytes;
use futures_util::TryStreamExt;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, error, info};

/// Shared application state
#[derive(Clone)]
struct AppState {
    manager: ProcessManager,
    model_service: Arc<ModelService>,
    default_context: u64,
}

/// Start the OpenAI-compatible proxy (blocking, for CLI use)
pub async fn start_proxy(
    host: String,
    port: u16,
    model_service: Arc<ModelService>,
    start_port: u16,
    default_context: u64,
) -> anyhow::Result<()> {
    // Ensure llama.cpp is installed
    crate::commands::llama::ensure_llama_initialized().await?;

    info!("Starting OpenAI-compatible proxy on {}:{}", host, port);
    info!(
        "llama-server instances will use ports starting from {}",
        start_port
    );
    info!("Default context size: {}", default_context);

    // Create ProcessManager with SingleSwap strategy
    let llama_server_path = crate::utils::paths::get_llama_server_path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "llama-server".to_string());

    let manager = ProcessManager::new_single_swap(Arc::clone(&model_service), start_port, llama_server_path);

    let state = AppState {
        manager: manager.clone(),
        model_service,
        default_context,
    };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(state);

    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr).await?;
    info!("Proxy listening on {}", addr);
    info!("Configure OpenWebUI to use: http://{}/v1", addr);

    // Graceful shutdown handler for CLI (Ctrl+C)
    let shutdown_manager = manager.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for ctrl-c");
        info!("Received shutdown signal, stopping models...");
        let _ = shutdown_manager.shutdown().await;
        let _ = shutdown_tx.send(());
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            shutdown_rx.await.ok();
        })
        .await?;

    Ok(())
}

/// Start the proxy server with custom shutdown signal (for GUI/background use)
/// Accepts a ProcessManager so GUI can manage its lifecycle
pub async fn start_proxy_with_shutdown(
    host: String,
    port: u16,
    manager: ProcessManager,
    default_context: u64,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    info!("Starting OpenAI-compatible proxy on {}:{}", host, port);
    info!("Default context size: {}", default_context);

    // Extract model_service from manager
    let model_service = manager.get_model_service().ok_or_else(|| {
        anyhow::anyhow!("ProcessManager must have SingleSwap strategy with model_service")
    })?;

    let state = AppState {
        manager: manager.clone(),
        model_service,
        default_context,
    };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(state);

    let addr = format!("{}:{}", host, port);

    // Bind to the address first to ensure it succeeds before spawning
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to bind proxy to {}: {}", addr, e))?;

    info!("Proxy listening on {}", addr);
    info!("Configure OpenWebUI to use: http://{}/v1", addr);

    let shutdown_manager = manager.clone();

    // Spawn the server in a background task
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown_rx.await.ok();
                info!("Proxy received shutdown signal, stopping models...");
                let _ = shutdown_manager.shutdown().await;
            })
            .await
        {
            error!("Proxy server error: {}", e);
        }
    });

    Ok(())
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok"
    }))
}

/// List all models from the database in OpenAI format
///
/// Note: Returns models sorted by added_at (descending) as provided by ModelService.
/// OpenAI API does not guarantee ordering, so this is acceptable.
async fn list_models(State(state): State<AppState>) -> impl IntoResponse {
    debug!("GET /v1/models");

    // Use ModelService to list models (no raw SQL in adapter layer)
    let models = state.model_service.list().await;

    match models {
        Ok(models) => {
            let model_infos: Vec<ModelInfo> = models
                .into_iter()
                .map(|m| ModelInfo {
                    id: m.name.clone(),
                    object: "model".to_string(),
                    created: m.added_at.timestamp(),
                    owned_by: "gglib".to_string(),
                    description: Some(format!(
                        "{} - {} parameters, {}",
                        m.architecture.as_deref().unwrap_or("unknown"),
                        m.param_count_b,
                        m.quantization.as_deref().unwrap_or("unknown quant")
                    )),
                })
                .collect();

            let response = ModelsResponse {
                object: "list".to_string(),
                data: model_infos,
            };

            Json(response).into_response()
        }
        Err(e) => {
            error!("Failed to list models: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    "Failed to list models",
                    "internal_error",
                )),
            )
                .into_response()
        }
    }
}

/// Handle chat completions - swap models and proxy to llama-server
async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    debug!("POST /v1/chat/completions");

    // Parse the request
    let request: ChatCompletionRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => {
            error!("Failed to parse request: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "Invalid request body",
                    "invalid_request",
                )),
            )
                .into_response();
        }
    };

    let model_name = request.model.clone();
    let is_streaming = request.stream;
    let num_ctx = request.num_ctx;

    info!(
        "Request for model '{}', streaming: {}, num_ctx: {:?}",
        model_name, is_streaming, num_ctx
    );

    // Ensure the model is running with specified context or default
    let port = match state
        .manager
        .ensure_model_running(&model_name, num_ctx, state.default_context)
        .await
    {
        Ok(port) => port,
        Err(e) => {
            error!("Failed to start model '{}': {}", model_name, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    format!("Failed to start model: {}", e),
                    "model_error",
                )),
            )
                .into_response();
        }
    };

    // Proxy the request to llama-server
    let upstream_url = format!("http://127.0.0.1:{}/v1/chat/completions", port);
    debug!("Proxying request to {}", upstream_url);

    let client = Client::builder(TokioExecutor::new()).build_http();

    // Build the upstream request
    let mut upstream_req = Request::builder()
        .method("POST")
        .uri(&upstream_url)
        .header("content-type", "application/json");

    // Copy relevant headers
    for (key, value) in headers.iter() {
        let key_str = key.as_str();
        if key_str != "host" && key_str != "content-length" {
            upstream_req = upstream_req.header(key, value);
        }
    }

    let upstream_req = match upstream_req.body(Body::from(body)) {
        Ok(req) => req,
        Err(e) => {
            error!("Failed to build upstream request: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    "Failed to build upstream request",
                    "internal_error",
                )),
            )
                .into_response();
        }
    };

    // Send request to llama-server
    match client.request(upstream_req).await {
        Ok(response) => {
            let status = response.status();
            let content_type = response.headers().get("content-type").cloned();

            // Convert hyper body to axum Body using http_body_util::StreamBody
            use http_body_util::{BodyExt, StreamBody};
            use hyper::body::Frame;
            let incoming_body = response.into_body();

            // Convert to a stream of Result<Frame<Bytes>, io::Error>
            let byte_stream = incoming_body
                .into_data_stream()
                .map_ok(Frame::data)
                .map_err(std::io::Error::other);

            let stream_body = StreamBody::new(byte_stream);
            let axum_body = Body::new(stream_body);

            // Build response with minimal headers
            let mut builder = Response::builder().status(status);

            // Only set essential streaming headers
            if is_streaming {
                builder = builder
                    .header("content-type", "text/event-stream")
                    .header("cache-control", "no-cache")
                    .header("x-accel-buffering", "no");
            } else {
                // For non-streaming, preserve content-type if present
                if let Some(ct) = content_type {
                    builder = builder.header("content-type", ct);
                }
            }

            builder.body(axum_body).unwrap()
        }
        Err(e) => {
            error!("Failed to proxy request to llama-server: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse::new(
                    format!("Failed to communicate with llama-server: {}", e),
                    "upstream_error",
                )),
            )
                .into_response()
        }
    }
}
