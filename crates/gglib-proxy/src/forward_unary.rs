//! The non-streaming half of `/v1/chat/completions`: one request up, one
//! body back, read whole, normalised and answered.
//!
//! Its own module because [`crate::forward`] sits at its complexity-ratchet
//! ceiling, and the two paths share nothing past the request shaping: the
//! streaming path drains frames through a channel, this one buffers a body.
//! The shared reading of that body lives in [`crate::unary_body`], which the
//! embeddings route uses too.

use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use tracing::{debug, error, warn};

use gglib_core::cache_metrics::CacheMetricsStore;
use gglib_core::domain::DialectSpec;

use crate::forward::ForwardError;
use crate::metrics::ContextMetricsStore;
use crate::models::ErrorResponse;
use crate::unary_body::forward_non_streaming_response;

/// Send one shaped, non-streaming chat completion upstream and answer with
/// its body.
///
/// The transport failures are mapped the way the streaming path maps them: a
/// connect failure or timeout is [`ForwardError::UpstreamDead`], so the caller
/// can recycle the server and answer a retriable 503; any other send error is
/// a terminal 502; and a non-2xx upstream status is passed through with its
/// body, since llama-server's own diagnostic is the useful one.
///
/// # Errors
///
/// [`ForwardError::UpstreamDead`] when llama-server could not be reached.
pub(crate) async fn forward_unary(
    req_builder: reqwest::RequestBuilder,
    body: Bytes,
    dialect: Option<&DialectSpec>,
    cache_metrics: &CacheMetricsStore,
    metrics: &ContextMetricsStore,
    snapshot_seq: u64,
) -> Result<Response, ForwardError> {
    let response = match req_builder.body(body).send().await {
        Ok(resp) => resp,
        Err(e) if e.is_connect() || e.is_timeout() => {
            // Connection refused or timed out — the llama-server process is dead
            // or hung.  Signal the caller so it can clear stale state and return
            // a retriable 503 rather than a terminal 502.
            error!("Upstream llama-server unreachable (connect/timeout): {e}");
            return Err(ForwardError::UpstreamDead);
        }
        Err(e) => {
            error!("Failed to send request to llama-server: {e}");
            return Ok((
                StatusCode::BAD_GATEWAY,
                axum::Json(ErrorResponse::upstream_error(&e.to_string())),
            )
                .into_response());
        }
    };

    let status = response.status();

    // For errors, return the error body directly
    if !status.is_success() {
        let error_bytes = response.bytes().await.unwrap_or_default();
        let error_body = String::from_utf8_lossy(&error_bytes);
        warn!(
            status = status.as_u16(),
            body = %error_body,
            "upstream llama-server returned error"
        );
        return Ok(Response::builder()
            .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
            .header("content-type", "application/json")
            .body(Body::from(error_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()));
    }

    debug!(
        status = status.as_u16(),
        "upstream llama-server accepted request"
    );

    // Non-streaming: read the full response and run it through the same
    // dialect normalization the streaming path applies.
    Ok(forward_non_streaming_response(
        response,
        cache_metrics,
        dialect,
        Some((metrics, snapshot_seq)),
    )
    .await)
}
