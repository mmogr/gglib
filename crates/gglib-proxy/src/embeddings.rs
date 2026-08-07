//! `POST /v1/embeddings` — OpenAI-compatible embeddings.
//!
//! Deliberately the chat path with everything chat-shaped removed. What it
//! keeps is what makes the endpoint part of the same gateway rather than a
//! second, parallel proxy: model resolution, admission control (so queueing, a
//! swap and its narration happen exactly as they do for chat), and the
//! dashboard registration that makes the traffic visible.
//!
//! What it does not keep, and why:
//!
//! | Chat machinery | Why it is absent here |
//! |---|---|
//! | History truncation, token calibration | No message history to size a budget against |
//! | `SamplingLayers` / inference defaults | Nothing is sampled |
//! | Session ids, KV slot save/restore | An embeddings request is stateless |
//! | SSE decode/normalize/encode | The endpoint does not stream |
//! | `{model}:{profile}` routing | Profiles only carry sampling parameters |
//! | Transparent restart-and-retry | That exists because the VS Code LLM Gateway treats 503 as terminal; an embeddings client has no such constraint, and a retry that re-embeds is the caller's to decide |
//!
//! ## Why this endpoint drove M9
//!
//! llama-server reads `--embeddings` as *restrict to only the embedding use
//! case*. gglib passes it for exactly the models tagged `embedding`, so an
//! embeddings server cannot answer chat completions and vice versa. With one
//! VRAM slot, a client doing both alternately paid for a full model swap on
//! every single request — the worst case admission control was built to fix.
//!
//! It is fixed from two directions, neither of them here: requests for the same
//! model are batched so one swap serves a burst, and an embedding model small
//! enough to co-reside takes the second slot and stops swapping altogether. See
//! [admission](gglib_runtime::process::admission) for both.

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use tracing::{debug, error, info};

use crate::dashboard::CacheStatus;
use crate::forward::{forward_non_streaming_response, should_forward_header};
use crate::models::{EmbeddingsRoutingEnvelope, ErrorResponse};
use crate::server::{AppState, handle_runtime_error};

/// The tag that marks a model as launchable in embedding mode.
///
/// Written at import time by `gglib_gguf::capabilities` and consumed by
/// `gglib_runtime`'s `resolve_embeddings_flag`. Read here so the proxy can
/// refuse a request the upstream would only 501 on, without paying for the
/// swap first.
pub const EMBEDDING_TAG: &str = "embedding";

/// Handle an embeddings request — ensure the model is running, then proxy.
pub(crate) async fn embeddings(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    debug!("POST /v1/embeddings");

    // Only `model` is parsed. Both `input` shapes — a bare string and an array
    // of strings — plus `encoding_format` and anything llama-server grows
    // later ride through untouched, because nothing here looks at them.
    let envelope: EmbeddingsRoutingEnvelope = match serde_json::from_slice(&body) {
        Ok(env) => env,
        Err(e) => {
            error!("Failed to parse embeddings request: {e}");
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::invalid_request(&format!(
                    "Invalid request body: {e}"
                ))),
            )
                .into_response();
        }
    };
    let model_name = envelope.model;

    // Resolved directly rather than through `request_pipeline::resolve`, which
    // collapses "not in the catalog" and "in the catalog" into one pass-through
    // context. Here the two need different answers: 404 for a model nobody has,
    // 400 for a model that exists but cannot do this.
    match state.catalog_port.resolve_model(&model_name).await {
        Ok(Some(summary)) => {
            if !summary.tags.iter().any(|t| t == EMBEDDING_TAG) {
                info!(
                    model = %model_name,
                    "refusing embeddings request for a model that is not an embedding model"
                );
                return (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::not_an_embedding_model(&model_name)),
                )
                    .into_response();
            }
        }
        Ok(None) => {
            // 404, matching what `handle_runtime_error` gives the chat path for
            // `ModelNotFound` — the same missing model must not report two
            // different statuses depending on which endpoint noticed.
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::model_not_found(&model_name)),
            )
                .into_response();
        }
        Err(e) => {
            error!(model = %model_name, error = %e, "catalog lookup failed");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::internal_error(&e.to_string())),
            )
                .into_response();
        }
    }

    info!(model = %model_name, "Processing embeddings request");

    // The same call the chat path makes, for the same reason: a request that
    // arrives mid-swap should wait it out rather than get a fast 503. `None`
    // for `num_ctx` — an embeddings request has no history to size a context
    // around, so it takes whatever the model is configured for.
    //
    // This is the endpoint the second resident slot exists for. An embedding
    // model small enough to co-reside is admitted here without displacing the
    // chat model at all, so the alternating traffic that used to cost a swap
    // per request now costs none.
    let admission = match state
        .runtime_port
        .admit(
            &model_name,
            None,
            state.default_ctx,
            gglib_core::ports::LaunchOverrides::default(),
        )
        .await
    {
        Ok(admission) => admission,
        Err(e) => return handle_runtime_error(e),
    };
    let target = admission.target.clone();

    // Same meeting point as the chat handler: the launch decided this in the
    // runtime, and the dashboard lives here. Skipping these two writes would
    // leave the cache and launch panels describing whatever ran before,
    // whenever an embedding model was the last thing loaded.
    state.dashboard.cache.set(CacheStatus::build(
        state.cache_enabled && state.slot_dir.is_some(),
        target.slot_restore_supported,
        target.cache_ram_health,
    ));
    if let Some(narration) = target.narration.clone() {
        state.dashboard.launch.set(narration);
    }

    // Registered so embeddings traffic appears in `/v1/proxy/status` rather
    // than looking like an idle proxy that is mysteriously busy. The guard
    // unregisters on drop, including on every early return below — and, since
    // it carries the admission lease, releases the model's VRAM slot with it.
    let _connection = state
        .dashboard
        .connections
        .register(model_name.clone(), false, Some(target.effective_ctx))
        .holding(admission.lease);

    let upstream_url = format!("{}/v1/embeddings", target.base_url);
    debug!(
        upstream = %upstream_url,
        model_id = %target.model_id,
        "Routing embeddings request to llama-server"
    );

    let mut req_builder = state
        .client
        .post(&upstream_url)
        .header("content-type", "application/json");
    for (name, value) in headers.iter() {
        if should_forward_header(name.as_str())
            && let Ok(value_str) = value.to_str()
        {
            req_builder = req_builder.header(name.as_str(), value_str);
        }
    }

    let response = match req_builder.body(body).send().await {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to send embeddings request to llama-server: {e}");
            return (
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse::upstream_error(&e.to_string())),
            )
                .into_response();
        }
    };

    let status = response.status();
    if !status.is_success() {
        // Passed through verbatim, including llama-server's own 501 for a
        // server that was not started with `--embeddings` — that message names
        // the real cause better than anything this layer could substitute.
        let error_bytes = response.bytes().await.unwrap_or_default();
        tracing::warn!(
            status = status.as_u16(),
            body = %String::from_utf8_lossy(&error_bytes),
            "upstream llama-server returned error for embeddings"
        );
        return Response::builder()
            .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(error_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    // No tags: an embeddings body has no `choices`, so normalization is a
    // no-op — this just satisfies the shared forwarding signature.
    forward_non_streaming_response(response, &state.dashboard.cache_metrics, None, None).await
}
