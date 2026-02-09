//! Axum HTTP server for the OpenAI-compatible AND Ollama-compatible proxy.
//!
//! Both API surfaces are served simultaneously:
//! - `/v1/*`   — OpenAI-compatible (existing)
//! - `/api/*`  — Ollama-native (new)
//! - `GET /`   — Ollama root probe

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use bytes::Bytes;
use reqwest::Client;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use gglib_core::ports::{ModelCatalogPort, ModelRuntimeError, ModelRuntimePort};

use crate::forward::forward_chat_completion;
use crate::models::{ChatCompletionRequest, ErrorResponse, ModelsResponse};
use crate::ollama_handlers::{self, ProxyState};
use crate::ollama_models::normalize_model_name;

/// Start the proxy server with a pre-bound listener.
pub async fn serve(
    listener: TcpListener,
    default_ctx: u64,
    runtime_port: Arc<dyn ModelRuntimePort>,
    catalog_port: Arc<dyn ModelCatalogPort>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let addr = listener.local_addr()?;
    info!("Proxy server starting on {addr}");

    let client = Client::builder().pool_max_idle_per_host(10).build()?;

    let state = ProxyState {
        client,
        runtime_port,
        catalog_port,
        default_ctx,
    };

    // OpenAI-compatible routes (existing)
    let openai_routes = Router::new()
        .route("/health", get(health_check))
        .route("/v1/models", get(list_models))
        .route("/v1/chat/completions", post(chat_completions))
        .with_state(state.clone());

    // Ollama-native routes (new)
    let ollama_routes = Router::new()
        .route("/", get(ollama_handlers::ollama_root))
        .route("/api/version", get(ollama_handlers::ollama_version))
        .route("/api/tags", get(ollama_handlers::ollama_tags))
        .route("/api/show", post(ollama_handlers::ollama_show))
        .route("/api/ps", get(ollama_handlers::ollama_ps))
        .route("/api/chat", post(ollama_handlers::ollama_chat))
        .route("/api/generate", post(ollama_handlers::ollama_generate))
        .route("/api/embed", post(ollama_handlers::ollama_embed))
        .route("/api/embeddings", post(ollama_handlers::ollama_embeddings_legacy))
        // Stubs for model management endpoints
        .route("/api/pull", post(ollama_handlers::ollama_pull))
        .route("/api/delete", delete(ollama_handlers::ollama_delete))
        .route("/api/copy", post(ollama_handlers::ollama_copy))
        .route("/api/create", post(ollama_handlers::ollama_create))
        .with_state(state);

    let app = openai_routes.merge(ollama_routes);

    info!("Proxy listening on {addr}");
    info!("OpenAI-compatible: http://{addr}/v1");
    info!("Ollama-compatible: http://{addr}/api");

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
async fn list_models(State(state): State<ProxyState>) -> impl IntoResponse {
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
    State(state): State<ProxyState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    debug!("POST /v1/chat/completions");

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

    // Apply Ollama model-name normalization on the OpenAI path too,
    // so clients mixing conventions still work.
    let model_name = normalize_model_name(&request.model).to_owned();
    let is_streaming = request.stream;
    let num_ctx = request.num_ctx;

    info!(
        model = %model_name,
        streaming = %is_streaming,
        num_ctx = ?num_ctx,
        "Processing chat completion request"
    );

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

    let upstream_url = format!("{}/v1/chat/completions", target.base_url);
    debug!(
        upstream = %upstream_url,
        model_id = %target.model_id,
        model_name = %target.model_name,
        "Routing to llama-server"
    );

    forward_chat_completion(&state.client, &upstream_url, &headers, body, is_streaming).await
}

/// Convert ModelRuntimeError to HTTP response with appropriate status code.
fn handle_runtime_error(err: ModelRuntimeError) -> Response {
    let (status, error_response) = match &err {
        ModelRuntimeError::ModelLoading => {
            (StatusCode::SERVICE_UNAVAILABLE, ErrorResponse::from(err))
        }
        ModelRuntimeError::ModelNotFound(_) => (StatusCode::NOT_FOUND, ErrorResponse::from(err)),
        ModelRuntimeError::ModelFileNotFound(_) => {
            (StatusCode::NOT_FOUND, ErrorResponse::from(err))
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, ErrorResponse::from(err)),
    };

    let mut response = (status, Json(error_response)).into_response();

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
