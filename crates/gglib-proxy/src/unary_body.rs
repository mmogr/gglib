//! A non-streaming response, read whole and normalised.
//!
//! The streaming path runs its frames through
//! [`gglib_core::normalize::NormalizingStream`] as they arrive; a `stream:
//! false` body is buffered anyway, so the same parser runs over it once
//! ([`gglib_core::normalize::normalize_chat_completion_body`]). Both
//! `/v1/chat/completions` and `/v1/embeddings` answer through here, which is
//! why it is not part of [`crate::forward_unary`].

use axum::{
    body::Body,
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use tracing::{error, warn};

use gglib_core::cache_metrics::CacheMetricsStore;
use gglib_core::domain::DialectSpec;

use crate::forward::NORMALIZATION_NOTICE_PREFIX;
use crate::metrics::ContextMetricsStore;
use crate::models::ErrorResponse;

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

/// Forward a non-streaming JSON response from llama-server, running the same
/// dialect normalization the streaming path applies.
///
/// `residue_sink` — the metrics store and this request's snapshot sequence
/// number — receives the dialect drift-alarm flag when post-normalization
/// content still carries dialect markup. `None` for paths that record no
/// snapshot (embeddings).
pub(crate) async fn forward_non_streaming_response(
    response: reqwest::Response,
    cache_metrics: &CacheMetricsStore,
    dialect: Option<&DialectSpec>,
    residue_sink: Option<(&ContextMetricsStore, u64)>,
) -> Response {
    // Collect upstream headers we want to preserve
    let content_type = response
        .headers()
        .get("content-type")
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("application/json"));

    // Read the full body
    match response.bytes().await {
        Ok(body_bytes) => {
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
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", content_type)
                .body(Body::from(body_bytes))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) => {
            error!("Failed to read upstream response: {e}");
            (
                StatusCode::BAD_GATEWAY,
                axum::Json(ErrorResponse::upstream_error(&e.to_string())),
            )
                .into_response()
        }
    }
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
