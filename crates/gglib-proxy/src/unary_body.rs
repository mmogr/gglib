//! A non-streaming request, sent within its bound, and its response, read
//! whole and normalised.
//!
//! The streaming path runs its frames through
//! [`gglib_core::normalize::NormalizingStream`] as they arrive; a `stream:
//! false` body is buffered anyway, so the same parser runs over it once
//! ([`gglib_core::normalize::normalize_chat_completion_body`]). Both
//! `/v1/chat/completions` and `/v1/embeddings` send through [`exchange`],
//! which is why it is not part of [`crate::forward_unary`].

use std::time::Duration;

use axum::{
    body::Body,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use tracing::{error, warn};

use gglib_core::cache_metrics::CacheMetricsStore;
use gglib_core::domain::DialectSpec;

use crate::forward::{FIRST_BYTE_DEADLINE_SECS, NORMALIZATION_NOTICE_PREFIX};
use crate::metrics::ContextMetricsStore;
use crate::models::ErrorResponse;

/// What a non-streaming answer is given for its generation, on top of what a
/// streamed reply is given to begin: 25 minutes, which is 15,000 tokens at 10
/// tokens a second, a long reasoning answer from a large model on slow
/// hardware.
const GENERATION_ALLOWANCE: Duration = Duration::from_mins(25);

/// How long a request that does not stream may take in all, from its send
/// upstream to the last byte of its answer: 30 minutes.
///
/// llama-server sends a non-streaming answer, headers and all, only once it
/// has generated the whole of it, so there is nothing to time in between: the
/// request is one wait, which a wedged llama-server would never end, and the
/// request would hold its model's slot all the while (#1125).
///
/// Sized from its twin on the streaming path, the time a streamed reply may
/// take to begin, [`FIRST_BYTE_DEADLINE_SECS`] (300 s), which is sized for a
/// long prefill on constrained hardware. A non-streaming answer cannot begin
/// until its generation is over, so it gets that and [`GENERATION_ALLOWANCE`]
/// besides: 300 s + 1500 s.
pub(crate) const UNARY_TOTAL_TIMEOUT: Duration =
    Duration::from_secs(FIRST_BYTE_DEADLINE_SECS).saturating_add(GENERATION_ALLOWANCE);

/// What one request that does not stream came to.
pub(crate) enum Exchange {
    /// llama-server answered 2xx: the content type to answer under, and the
    /// body, read whole and normalised.
    Answered(HeaderValue, Bytes),
    /// The response to send instead: llama-server's own error passed through,
    /// a 502 for a body that could not be read, or a 504 for a request that
    /// outlasted its bound.
    Respond(Response),
    /// The request failed before llama-server answered it.
    SendFailed(reqwest::Error),
}

/// Send `request`, and read its answer whole as [`read_non_streaming_body`]
/// does, all within `total`.
///
/// When `total` runs out first, the request is dropped, which closes its
/// connection to llama-server as a departed client's does, and the answer is
/// a 504 carrying `upstream_timeout`: the code the streaming path sends when
/// its upstream goes quiet (see [`crate::upstream_read`]), so a client reads
/// both the same way.
pub(crate) async fn exchange(
    request: reqwest::RequestBuilder,
    total: Duration,
    cache_metrics: &CacheMetricsStore,
    dialect: Option<&DialectSpec>,
    residue_sink: Option<(&ContextMetricsStore, u64)>,
) -> Exchange {
    let exchange = async {
        let response = match request.send().await {
            Ok(response) => response,
            Err(e) => return Exchange::SendFailed(e),
        };
        let status = response.status();
        if !status.is_success() {
            return Exchange::Respond(passed_through(status, response).await);
        }
        match read_non_streaming_body(response, cache_metrics, dialect, residue_sink).await {
            Ok((content_type, body)) => Exchange::Answered(content_type, body),
            Err(e) => Exchange::Respond(unreadable_upstream(&e)),
        }
    };
    tokio::time::timeout(total, exchange)
        .await
        .unwrap_or_else(|_| Exchange::Respond(upstream_timed_out(total)))
}

/// llama-server's own error, its status and body passed through: its
/// diagnostic, such as the 501 of a server not started with `--embeddings`,
/// names the cause better than anything this layer could put in its place.
async fn passed_through(status: reqwest::StatusCode, response: reqwest::Response) -> Response {
    let error_bytes = response.bytes().await.unwrap_or_default();
    warn!(
        status = status.as_u16(),
        body = %String::from_utf8_lossy(&error_bytes),
        "upstream llama-server returned error"
    );
    Response::builder()
        .status(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY))
        .header("content-type", "application/json")
        .body(Body::from(error_bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// The 504 for a request that outlasted `total`.
fn upstream_timed_out(total: Duration) -> Response {
    let secs = total.as_secs();
    warn!(
        secs,
        "upstream did not finish a non-streaming answer within {secs}s"
    );
    (
        StatusCode::GATEWAY_TIMEOUT,
        axum::Json(ErrorResponse::with_code(
            format!("upstream did not finish its answer within {secs}s"),
            "server_error",
            "upstream_timeout",
        )),
    )
        .into_response()
}

/// Extract `(prompt_tokens, cached_tokens)` from a non-streaming response body.
///
/// The streaming path gets these from a typed `Usage` event; a non-streaming
/// response carries the same figures in its terminal JSON instead, so they are
/// read here rather than leaving this path silently absent from the telemetry.
///
/// Returns `None` when the body isn't JSON or carries no `usage.prompt_tokens`
/// — nothing is recorded in that case, rather than recording a zero that would
/// dilute the totals. The inner `cached_tokens` stays `Option` for the reason
/// given on [`gglib_core::LlmStreamEvent::Usage`]: absent and zero differ.
fn usage_from_response_body(body: &[u8]) -> Option<(u32, Option<u32>)> {
    let parsed: serde_json::Value = serde_json::from_slice(body).ok()?;
    let usage = parsed.get("usage")?;
    let prompt_tokens = u32::try_from(usage.get("prompt_tokens")?.as_u64()?).ok()?;
    let cached_tokens = usage
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(serde_json::Value::as_u64)
        .map(|v| u32::try_from(v).unwrap_or(u32::MAX));
    Some((prompt_tokens, cached_tokens))
}

/// Read a non-streaming answer whole and run it through the dialect parser
/// once: the content type to answer under, and the normalised body.
///
/// `residue_sink` — the metrics store and this request's snapshot sequence
/// number — receives the dialect drift-alarm flag when post-normalization
/// content still carries dialect markup. `None` for paths that record no
/// snapshot (embeddings).
///
/// # Errors
///
/// The transport error, when the body could not be read; answer with
/// [`unreadable_upstream`].
async fn read_non_streaming_body(
    response: reqwest::Response,
    cache_metrics: &CacheMetricsStore,
    dialect: Option<&DialectSpec>,
    residue_sink: Option<(&ContextMetricsStore, u64)>,
) -> reqwest::Result<(HeaderValue, Bytes)> {
    // Collect upstream headers we want to preserve
    let content_type = response
        .headers()
        .get("content-type")
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("application/json"));

    // Read the full body
    let body_bytes = response.bytes().await?;
    // Body is already fully buffered, so this is a parse of bytes we
    // hold rather than extra I/O. Failure is silent by design: an
    // unparseable body still forwards verbatim, since telemetry must
    // never change what the client receives.
    if let Some((prompt_tokens, cached_tokens)) = usage_from_response_body(&body_bytes) {
        cache_metrics.record(prompt_tokens, cached_tokens);
    }
    let (body_bytes, residue) = normalize_non_streaming_body(body_bytes, dialect);
    if let (Some(marker), Some((metrics, seq))) = (residue, residue_sink) {
        warn!(
            marker = %marker,
            "dialect residue reached client-visible output (non-streaming)"
        );
        metrics.flag_dialect_residue(seq);
    }
    Ok((content_type, body_bytes))
}

/// A 200 carrying `body` under `content_type`.
pub(crate) fn answer_with(content_type: HeaderValue, body: Bytes) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", content_type)
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// The 502 for an upstream body that could not be read.
fn unreadable_upstream(e: &reqwest::Error) -> Response {
    error!("Failed to read upstream response: {e}");
    (
        StatusCode::BAD_GATEWAY,
        axum::Json(ErrorResponse::upstream_error(&e.to_string())),
    )
        .into_response()
}

/// Run dialect normalization over a buffered non-streaming body.
///
/// The same parser the streaming path uses, driven once over the complete
/// content (`gglib_core::normalize::normalize_chat_completion_body`), so a
/// `stream: false` client gets structured `tool_calls` instead of raw
/// dialect markup. Parse failures forward the original bytes verbatim — a
/// body we cannot read is a body we must not rewrite. Normalization errors
/// get the same treatment as on the streaming path: logged, and the raw
/// markup surfaced as visibly-flagged assistant text rather than silently
/// dropped.
///
/// The second return value is the drift alarm's one-shot scan of each
/// choice's post-normalization content: the first dialect marker that
/// survived into client-visible text, if any.
pub(crate) fn normalize_non_streaming_body(
    body_bytes: Bytes,
    dialect: Option<&DialectSpec>,
) -> (Bytes, Option<String>) {
    let Ok(mut parsed) = serde_json::from_slice::<serde_json::Value>(&body_bytes) else {
        return (body_bytes, None);
    };

    let errors = gglib_core::normalize::normalize_chat_completion_body(&mut parsed, dialect);

    for err in &errors {
        warn!(?err, "normalization error in non-streaming response");
    }
    if !errors.is_empty()
        && let Some(content) = parsed
            .get_mut("choices")
            .and_then(|c| c.get_mut(0))
            .and_then(|c| c.get_mut("message"))
            .and_then(|m| m.get_mut("content"))
    {
        let mut text = content.as_str().unwrap_or_default().to_owned();
        for err in &errors {
            text.push_str(NORMALIZATION_NOTICE_PREFIX);
            text.push_str(&err.raw);
        }
        *content = serde_json::Value::String(text);
    }

    // Drift alarm: one-shot scan of each choice's post-normalization
    // content. Skipped when normalization errors were surfaced — their
    // recovery notice embeds the raw markup and already flags itself.
    let residue = if errors.is_empty() {
        parsed
            .get("choices")
            .and_then(serde_json::Value::as_array)
            .and_then(|choices| {
                choices.iter().find_map(|choice| {
                    choice
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(serde_json::Value::as_str)
                        .and_then(|text| gglib_core::normalize::scan_complete(text, dialect))
                })
            })
    } else {
        None
    };

    (
        serde_json::to_vec(&parsed).map_or(body_bytes, Bytes::from),
        residue,
    )
}
