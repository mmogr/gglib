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
use tracing::{debug, error, info};

use gglib_core::ports::{ModelCatalogPort, ModelRuntimeError, ModelRuntimePort};

use crate::forward::forward_chat_completion;
use crate::models::{ChatCompletionRequest, ErrorResponse, ModelsResponse};

/// Shared application state for the proxy server.
#[derive(Clone)]
struct AppState {
    /// HTTP client for forwarding requests to llama-server.
    client: Client,
    /// Port for managing model runtime.
    runtime_port: Arc<dyn ModelRuntimePort>,
    /// Port for listing and resolving models.
    catalog_port: Arc<dyn ModelCatalogPort>,
    /// Default context size when not specified in request.
    default_ctx: u64,
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
/// * `cancel` - Cancellation token for graceful shutdown
///
/// # Returns
///
/// Returns `Ok(())` on clean shutdown, or an error if the server fails.
pub async fn serve(
    listener: TcpListener,
    default_ctx: u64,
    runtime_port: Arc<dyn ModelRuntimePort>,
    catalog_port: Arc<dyn ModelCatalogPort>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let addr = listener.local_addr()?;
    info!("Proxy server starting on {addr}");

    // Create HTTP client for upstream requests
    let client = Client::builder().pool_max_idle_per_host(10).build()?;

    let state = AppState {
        client,
        runtime_port,
        catalog_port,
        default_ctx,
    };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(state);

    info!("Proxy listening on {addr}");
    info!("Configure OpenWebUI to use: http://{addr}/v1");

    axum::serve(listener, app)
        .with_graceful_shutdown(cancel.cancelled_owned())
        .await?;

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
async fn list_models(State(state): State<AppState>) -> impl IntoResponse {
    debug!("GET /v1/models");

    match state.catalog_port.list_models().await {
        Ok(models) => {
            let response = ModelsResponse::from_summaries(models);
            Json(response).into_response()
        }
        Err(e) => {
            error!("Failed to list models: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    format!("Failed to list models: {e}"),
                    "internal_error",
                )),
            )
                .into_response()
        }
    }
}

/// Handle chat completions - ensure model is running and proxy to llama-server.
async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    debug!("POST /v1/chat/completions");

    // Parse the request to extract model name and streaming flag
    let request: ChatCompletionRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => {
            error!("Failed to parse request: {e}");
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    format!("Invalid request body: {e}"),
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
        model = %model_name,
        streaming = %is_streaming,
        num_ctx = ?num_ctx,
        "Processing chat completion request"
    );

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

    // Forward the request
    forward_chat_completion(&state.client, &upstream_url, &headers, body, is_streaming).await
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
