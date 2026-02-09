//! Ollama-native API route handlers.
//!
//! These handlers accept Ollama-format requests, translate them to the
//! internal OpenAI format, forward to llama-server, and translate the
//! response back into Ollama format. This makes the proxy a drop-in
//! replacement for Ollama on port 11434.

use std::sync::Arc;
use std::time::Instant;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, error, info, warn};

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort, RunningTarget};

use crate::ollama_models::*;
use crate::ollama_stream;

// ── Shared State ───────────────────────────────────────────────────────

/// Shared proxy state — cloneable, injected via Axum `State`.
///
/// Used by both the OpenAI (`/v1/`) and Ollama (`/api/`) route trees.
#[derive(Clone)]
pub(crate) struct ProxyState {
    pub client: Client,
    pub runtime_port: Arc<dyn ModelRuntimePort>,
    pub catalog_port: Arc<dyn ModelCatalogPort>,
    pub default_ctx: u64,
}

// ── GET / ──────────────────────────────────────────────────────────────

/// Ollama root probe — returns `"Ollama is running"` (plain text).
pub(crate) async fn ollama_root() -> impl IntoResponse {
    OLLAMA_ROOT_RESPONSE
}

// ── GET /api/version ───────────────────────────────────────────────────

pub(crate) async fn ollama_version() -> impl IntoResponse {
    // Return an Ollama-compatible version number to satisfy client requirements.
    // VSCode Ollama extension requires >= 0.6.4. We return 0.6.4 to indicate
    // compatibility while maintaining a stable version claim.
    Json(OllamaVersionResponse {
        version: "0.6.4".to_string(),
    })
}

// ── GET /api/tags ──────────────────────────────────────────────────────

pub(crate) async fn ollama_tags(State(state): State<ProxyState>) -> impl IntoResponse {
    debug!("GET /api/tags");
    match state.catalog_port.list_models().await {
        Ok(models) => Json(OllamaTagsResponse::from_summaries(models)).into_response(),
        Err(e) => {
            error!("Failed to list models: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ollama_error(&e.to_string())))
                .into_response()
        }
    }
}

// ── POST /api/show ─────────────────────────────────────────────────────

pub(crate) async fn ollama_show(
    State(state): State<ProxyState>,
    Json(req): Json<OllamaShowRequest>,
) -> impl IntoResponse {
    let name = normalize_model_name(&req.name).to_owned();
    debug!(model = %name, "POST /api/show");

    match state.catalog_port.resolve_model(&name).await {
        Ok(Some(summary)) => Json(OllamaShowResponse::from_summary(&summary)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ollama_error(&format!("model '{name}' not found"))),
        )
            .into_response(),
        Err(e) => {
            error!("Failed to resolve model: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ollama_error(&e.to_string())))
                .into_response()
        }
    }
}

// ── GET /api/ps ────────────────────────────────────────────────────────

pub(crate) async fn ollama_ps(State(state): State<ProxyState>) -> impl IntoResponse {
    debug!("GET /api/ps");
    let models = match state.runtime_port.current_model().await {
        Some(target) => vec![running_target_to_ps_entry(&target)],
        None => vec![],
    };
    Json(OllamaPsResponse { models })
}

fn running_target_to_ps_entry(target: &RunningTarget) -> OllamaPsEntry {
    OllamaPsEntry {
        name: format!("{}:latest", target.model_name),
        model: format!("{}:latest", target.model_name),
        size: 0,
        digest: format!("{:016x}", target.model_id as u64),
        details: OllamaModelDetails {
            parent_model: String::new(),
            format: "gguf".to_string(),
            family: String::new(),
            families: vec![],
            parameter_size: String::new(),
            quantization_level: String::new(),
        },
        expires_at: "0001-01-01T00:00:00Z".to_string(),
        size_vram: 0,
    }
}

// ── POST /api/chat ─────────────────────────────────────────────────────

pub(crate) async fn ollama_chat(
    State(state): State<ProxyState>,
    body: Bytes,
) -> Response {
    let req: OllamaChatRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            error!("Invalid /api/chat request: {e}");
            return (StatusCode::BAD_REQUEST, Json(ollama_error(&e.to_string()))).into_response();
        }
    };

    let model_name = normalize_model_name(&req.model).to_owned();
    let is_streaming = req.stream;
    let num_ctx = req.options.num_ctx;

    info!(model = %model_name, streaming = %is_streaming, "POST /api/chat");

    let target = match ensure_model(&state, &model_name, num_ctx).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    let upstream_url = format!("{}/v1/chat/completions", target.base_url);
    let start = Instant::now();

    // Build the OpenAI body with stream set correctly for the desired mode.
    let openai_body = build_openai_chat_body(&req, &model_name, is_streaming);

    let upstream_resp = match forward_post(&state.client, &upstream_url, &openai_body).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    if is_streaming {
        ollama_stream::stream_chat_response(upstream_resp, model_name, start).await
    } else {
        non_streaming_chat_response(upstream_resp, &model_name, start).await
    }
}

/// Apply Ollama-style options to an OpenAI JSON body.
fn apply_openai_options(body: &mut serde_json::Value, options: &OllamaOptions) {
    if let Some(temp) = options.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(top_p) = options.top_p {
        body["top_p"] = serde_json::json!(top_p);
    }
    if let Some(num_predict) = options.num_predict {
        if num_predict > 0 {
            body["max_tokens"] = serde_json::json!(num_predict);
        }
    }
    if let Some(ref stop) = options.stop {
        body["stop"] = serde_json::json!(stop);
    }
}

/// Build an OpenAI `/v1/chat/completions` JSON body from an Ollama chat request.
///
/// `upstream_stream` controls the `stream` field sent to llama-server:
/// - `true` when the Ollama client requested streaming (we need SSE chunks to translate)
/// - `false` when the Ollama client requested non-streaming (we get a single JSON response)
fn build_openai_chat_body(
    req: &OllamaChatRequest,
    normalized_model: &str,
    upstream_stream: bool,
) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = req
        .messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content,
            })
        })
        .collect();

    let mut body = serde_json::json!({
        "model": normalized_model,
        "messages": messages,
        "stream": upstream_stream,
    });

    apply_openai_options(&mut body, &req.options);

    body
}

/// Parsed fields from an upstream OpenAI chat-completion response.
struct UpstreamCompletion {
    content: String,
    finish_reason: String,
    prompt_tokens: u32,
    completion_tokens: u32,
}

/// Read and parse a non-streaming OpenAI upstream response.
async fn parse_upstream_completion(response: reqwest::Response) -> Result<UpstreamCompletion, Response> {
    let bytes = response.bytes().await.map_err(|e| {
        error!("Failed to read upstream response: {e}");
        (StatusCode::BAD_GATEWAY, Json(ollama_error(&e.to_string()))).into_response()
    })?;

    let openai: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
        error!("Failed to parse upstream JSON: {e}");
        (StatusCode::BAD_GATEWAY, Json(ollama_error(&e.to_string()))).into_response()
    })?;

    Ok(UpstreamCompletion {
        content: openai["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        finish_reason: openai["choices"][0]["finish_reason"]
            .as_str()
            .unwrap_or("stop")
            .to_string(),
        prompt_tokens: openai["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
        completion_tokens: openai["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
    })
}

/// Non-streaming: read the full OpenAI response and translate to Ollama format.
async fn non_streaming_chat_response(
    response: reqwest::Response,
    model: &str,
    start: Instant,
) -> Response {
    let upstream = match parse_upstream_completion(response).await {
        Ok(u) => u,
        Err(resp) => return resp,
    };

    let total_nanos = elapsed_nanos(start);

    let resp = OllamaChatResponse {
        model: model.to_string(),
        created_at: now_rfc3339(),
        message: OllamaChatMessage {
            role: "assistant".to_string(),
            content: upstream.content,
            images: None,
            tool_calls: None,
        },
        done: true,
        done_reason: Some(upstream.finish_reason),
        total_duration: total_nanos,
        load_duration: 0,
        prompt_eval_count: upstream.prompt_tokens,
        prompt_eval_duration: total_nanos / 4,
        eval_count: upstream.completion_tokens,
        eval_duration: total_nanos * 3 / 4,
    };

    Json(resp).into_response()
}

// ── POST /api/generate ─────────────────────────────────────────────────

pub(crate) async fn ollama_generate(
    State(state): State<ProxyState>,
    body: Bytes,
) -> Response {
    let req: OllamaGenerateRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            error!("Invalid /api/generate request: {e}");
            return (StatusCode::BAD_REQUEST, Json(ollama_error(&e.to_string()))).into_response();
        }
    };

    let model_name = normalize_model_name(&req.model).to_owned();
    let is_streaming = req.stream;
    let num_ctx = req.options.num_ctx;

    info!(model = %model_name, streaming = %is_streaming, "POST /api/generate");

    let target = match ensure_model(&state, &model_name, num_ctx).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    // Build OpenAI chat body (generate maps prompt → a single user message).
    let mut messages = Vec::new();
    if let Some(ref sys) = req.system {
        messages.push(serde_json::json!({"role": "system", "content": sys}));
    }
    messages.push(serde_json::json!({"role": "user", "content": req.prompt}));

    let mut openai_body = serde_json::json!({
        "model": model_name,
        "messages": messages,
        "stream": is_streaming,
    });
    apply_openai_options(&mut openai_body, &req.options);

    let upstream_url = format!("{}/v1/chat/completions", target.base_url);
    let start = Instant::now();

    let upstream_resp = match forward_post(&state.client, &upstream_url, &openai_body).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    if is_streaming {
        ollama_stream::stream_generate_response(upstream_resp, model_name, start).await
    } else {
        non_streaming_generate_response(upstream_resp, &model_name, start).await
    }
}

async fn non_streaming_generate_response(
    response: reqwest::Response,
    model: &str,
    start: Instant,
) -> Response {
    let upstream = match parse_upstream_completion(response).await {
        Ok(u) => u,
        Err(resp) => return resp,
    };

    let total_nanos = elapsed_nanos(start);

    let resp = OllamaGenerateResponse {
        model: model.to_string(),
        created_at: now_rfc3339(),
        response: upstream.content,
        done: true,
        done_reason: Some("stop".to_string()),
        total_duration: total_nanos,
        load_duration: 0,
        prompt_eval_count: upstream.prompt_tokens,
        prompt_eval_duration: total_nanos / 4,
        eval_count: upstream.completion_tokens,
        eval_duration: total_nanos * 3 / 4,
        context: None,
    };

    Json(resp).into_response()
}

// ── POST /api/embed ────────────────────────────────────────────────────

pub(crate) async fn ollama_embed(
    State(state): State<ProxyState>,
    body: Bytes,
) -> Response {
    let req: OllamaEmbeddingRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            error!("Invalid /api/embed request: {e}");
            return (StatusCode::BAD_REQUEST, Json(ollama_error(&e.to_string()))).into_response();
        }
    };

    let model_name = normalize_model_name(&req.model).to_owned();
    let num_ctx = req.options.num_ctx;
    info!(model = %model_name, "POST /api/embed");

    let target = match ensure_model(&state, &model_name, num_ctx).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    let input_vec = req.input.into_vec();
    let openai_body = serde_json::json!({
        "model": model_name,
        "input": input_vec,
    });

    let upstream_url = format!("{}/v1/embeddings", target.base_url);
    let start = Instant::now();

    let upstream_resp = match forward_post(&state.client, &upstream_url, &openai_body).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let bytes = match upstream_resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            error!("Failed to read upstream embedding response: {e}");
            return (StatusCode::BAD_GATEWAY, Json(ollama_error(&e.to_string()))).into_response();
        }
    };

    let openai_resp: OpenAiEmbeddingResponse = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse upstream embedding JSON: {e}");
            return (StatusCode::BAD_GATEWAY, Json(ollama_error(&e.to_string()))).into_response();
        }
    };

    let total_nanos = elapsed_nanos(start);
    let prompt_tokens = openai_resp.usage.as_ref().map(|u| u.prompt_tokens);

    let embeddings: Vec<Vec<f32>> = openai_resp
        .data
        .into_iter()
        .map(|d| d.embedding)
        .collect();

    let resp = OllamaEmbeddingResponse {
        model: model_name,
        embeddings,
        total_duration: Some(total_nanos),
        load_duration: Some(0),
        prompt_eval_count: prompt_tokens,
    };

    Json(resp).into_response()
}

// ── POST /api/embeddings (legacy) ──────────────────────────────────────

/// Legacy `/api/embeddings` — returns a single embedding vector.
pub(crate) async fn ollama_embeddings_legacy(
    State(state): State<ProxyState>,
    body: Bytes,
) -> Response {
    #[derive(Deserialize)]
    struct LegacyReq {
        model: String,
        prompt: String,
        #[serde(default)]
        options: OllamaOptions,
    }

    let req: LegacyReq = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            error!("Invalid /api/embeddings request: {e}");
            return (StatusCode::BAD_REQUEST, Json(ollama_error(&e.to_string()))).into_response();
        }
    };

    let model_name = normalize_model_name(&req.model).to_owned();
    info!(model = %model_name, "POST /api/embeddings (legacy)");

    let target = match ensure_model(&state, &model_name, req.options.num_ctx).await {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    let openai_body = serde_json::json!({
        "model": model_name,
        "input": [req.prompt],
    });

    let upstream_url = format!("{}/v1/embeddings", target.base_url);

    let upstream_resp = match forward_post(&state.client, &upstream_url, &openai_body).await {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let bytes = match upstream_resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            error!("Failed to read upstream embedding response: {e}");
            return (StatusCode::BAD_GATEWAY, Json(ollama_error(&e.to_string()))).into_response();
        }
    };

    let openai_resp: OpenAiEmbeddingResponse = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse upstream embedding JSON: {e}");
            return (StatusCode::BAD_GATEWAY, Json(ollama_error(&e.to_string()))).into_response();
        }
    };

    let embedding = openai_resp
        .data
        .into_iter()
        .next()
        .map(|d| d.embedding)
        .unwrap_or_default();

    Json(OllamaLegacyEmbeddingResponse { embedding }).into_response()
}

// ── Unsupported management stubs ───────────────────────────────────────

pub(crate) async fn ollama_pull() -> impl IntoResponse {
    warn!("POST /api/pull — model management is handled by gglib CLI");
    (
        StatusCode::NOT_FOUND,
        Json(ollama_error(
            "Model pulling is not supported via the Ollama API. Use `gglib add <model>` instead.",
        )),
    )
}

pub(crate) async fn ollama_delete() -> impl IntoResponse {
    warn!("DELETE /api/delete — model management is handled by gglib CLI");
    (
        StatusCode::NOT_FOUND,
        Json(ollama_error(
            "Model deletion is not supported via the Ollama API. Use `gglib rm <model>` instead.",
        )),
    )
}

pub(crate) async fn ollama_copy() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(ollama_error("Model copying is not supported.")),
    )
}

pub(crate) async fn ollama_create() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(ollama_error("Modelfile creation is not supported.")),
    )
}

// ── Shared Helpers ─────────────────────────────────────────────────────

/// Ensure the model is running, returning the target or an Ollama-format error response.
pub(crate) async fn ensure_model(
    state: &ProxyState,
    model: &str,
    num_ctx: Option<u64>,
) -> Result<RunningTarget, Response> {
    state
        .runtime_port
        .ensure_model_running(model, num_ctx, state.default_ctx)
        .await
        .map_err(|e| {
            let status = StatusCode::from_u16(e.suggested_status_code())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let mut resp = (status, Json(ollama_error(&e.to_string()))).into_response();
            if e.is_retryable() {
                if let Ok(val) = "5".parse() {
                    resp.headers_mut().insert("retry-after", val);
                }
            }
            resp
        })
}

/// POST JSON to upstream llama-server and return the response.
async fn forward_post(
    client: &Client,
    url: &str,
    body: &serde_json::Value,
) -> Result<reqwest::Response, Response> {
    debug!("Forwarding to {url}");
    let response = client
        .post(url)
        .header("content-type", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|e| {
            error!("Failed to connect to llama-server: {e}");
            (
                StatusCode::BAD_GATEWAY,
                Json(ollama_error(&format!("Failed to connect to model server: {e}"))),
            )
                .into_response()
        })?;

    let status = response.status();
    if !status.is_success() {
        let err_bytes = response.bytes().await.unwrap_or_default();
        let msg = String::from_utf8_lossy(&err_bytes);
        error!("Upstream error {status}: {msg}");
        return Err((
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            Json(ollama_error(&format!("Upstream error: {msg}"))),
        )
            .into_response());
    }

    Ok(response)
}

/// Build an Ollama-style error JSON object: `{"error": "message"}`.
fn ollama_error(msg: &str) -> serde_json::Value {
    serde_json::json!({ "error": msg })
}
