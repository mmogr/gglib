//! Streaming SSE spawn + KV cache lifecycle hooks, relocated from forward.rs.

use std::sync::Arc;

use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use tracing::{debug, error, warn};

use crate::cache_lifecycle::{StreamConfig, save_after_generation};
use crate::connections::ConnectionGuard;
use crate::forward::{FIRST_BYTE_DEADLINE_SECS, stream_response_to_channel, visible_content_frame};
use crate::token_calibration::TokenCalibration;
use crate::upstream_health::UpstreamHealth;
use gglib_core::cache_metrics::CacheMetricsStore;

/// Maximum number of retry attempts for the pre-generation connection phase
/// (TCP send / first-byte-deadline wait) before falling back to an inline
/// error frame. Total attempts = 1 (initial) + MAX_RETRIES = 3.
const MAX_RETRIES: u32 = 2;

/// Backoff between pre-generation retry attempts (100ms).
const RETRY_BACKOFF: std::time::Duration = std::time::Duration::from_millis(100);

/// Spawn the detached keepalive/streaming task and return the immediate SSE
/// response — relocated verbatim from `forward_chat_completion`'s streaming
/// branch (Step 4).
///
/// Bounds the pre-generation connection-establishment phase (TCP send /
/// first-byte-deadline wait) with a bounded retry (`MAX_RETRIES` attempts,
/// `RETRY_BACKOFF` apart). Retries stop the moment a response is obtained —
/// once [`stream_response_to_channel`] begins draining a successful
/// response, no further retries occur; mid-stream failures become inline
/// error frames, exactly as before.
///
/// When `config` and `session_id` are both `Some` (KV cache enabled), the KV
/// cache is saved via [`save_after_generation`] immediately after
/// [`stream_response_to_channel`] returns — before the semaphore `permit`
/// drops at the end of this task.
#[allow(clippy::too_many_arguments)]
pub fn spawn_and_return(
    req_builder: reqwest::RequestBuilder,
    body: Bytes,
    tx: tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    rx: tokio::sync::mpsc::Receiver<Result<Bytes, std::io::Error>>,
    connection: ConnectionGuard,
    model_name_owned: String,
    tags: Vec<String>,
    upstream_health: Arc<UpstreamHealth>,
    calibration: Arc<TokenCalibration>,
    cache_metrics: Arc<CacheMetricsStore>,
    forwarded_chars: usize,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
    config: Option<StreamConfig>,
    session_id: Option<String>,
) -> Response {
    // `connection` is moved into this task so it lives exactly as long
    // as the streaming task does — dropped (unregistering from the
    // dashboard) whether the task finishes normally, the client
    // disconnects (the task is a detached `tokio::spawn`, but `tx` being
    // dropped ends the response body stream, and the task itself exits
    // once `stream_response_to_channel` observes the closed channel), or
    // panics.
    tokio::spawn(async move {
        let connection = connection;
        // KV cache semaphore gate (if cache is enabled) — held for this
        // task's entire lifetime, dropped implicitly when this async block
        // ends on every exit path (success, error-frame-and-return, or
        // panic). No explicit use is needed.
        let _permit = permit;
        let mut keepalive_interval = tokio::time::interval(std::time::Duration::from_secs(15));
        keepalive_interval.tick().await; // skip first immediate tick

        let mut retries: u32 = 0;
        let upstream_response = 'retry: loop {
            // Race: llama.cpp response headers vs 15-second keepalive timer.
            let send_future = req_builder
                .try_clone()
                .expect("streaming request body is Bytes (non-stream); try_clone always succeeds")
                .body(body.clone())
                .send();
            tokio::pin!(send_future);

            // Overall first-byte deadline: bounds pathological slot-queue
            // waits so a wedged upstream cannot hang the client indefinitely
            // on keepalive comments.
            let deadline =
                tokio::time::sleep(std::time::Duration::from_secs(FIRST_BYTE_DEADLINE_SECS));
            tokio::pin!(deadline);

            let attempt_result = loop {
                tokio::select! {
                    biased;
                    result = &mut send_future => break result,
                    () = &mut deadline => {
                        // The single-slot upstream may legitimately be busy
                        // serving another (possibly minutes-long) request, in
                        // which case this request is correctly queued, not
                        // wedged. Only treat a deadline expiry as degradation
                        // when NO other connection is actively occupying the
                        // slot; otherwise extend the deadline and keep waiting.
                        if connection.others_active() {
                            warn!(
                                deadline_secs = FIRST_BYTE_DEADLINE_SECS,
                                "slot-queue wait exceeded deadline but another request is active; extending (upstream busy, not wedged)"
                            );
                            deadline.as_mut().reset(
                                tokio::time::Instant::now()
                                    + std::time::Duration::from_secs(FIRST_BYTE_DEADLINE_SECS),
                            );
                            continue;
                        }
                        if retries < MAX_RETRIES {
                            retries += 1;
                            warn!(
                                deadline_secs = FIRST_BYTE_DEADLINE_SECS,
                                retries,
                                "slot-queue wait exceeded first-byte deadline; retrying pre-generation phase"
                            );
                            tokio::time::sleep(RETRY_BACKOFF).await;
                            continue 'retry;
                        }
                        warn!(
                            deadline_secs = FIRST_BYTE_DEADLINE_SECS,
                            "slot-queue wait exceeded first-byte deadline with no other active request; treating upstream as degraded"
                        );
                        upstream_health.record_timeout();
                        let visible = visible_content_frame(
                            &model_name_owned,
                            &format!(
                                "⚠️ [proxy] upstream model server did not begin responding within {FIRST_BYTE_DEADLINE_SECS}s — it may be overloaded or wedged. Retry; if it persists the model will be recycled."
                            ),
                        );
                        let payload = serde_json::json!({
                            "error": {
                                "message": format!(
                                    "upstream did not respond within {FIRST_BYTE_DEADLINE_SECS}s"
                                ),
                                "type": "server_error",
                                "code": "upstream_timeout",
                            }
                        });
                        let frame = format!("{visible}data: {payload}\n\ndata: [DONE]\n\n");
                        let _ = tx.send(Ok(Bytes::from(frame))).await;
                        return;
                    }
                    _ = keepalive_interval.tick() => {
                        debug!("slot-queue wait: sending SSE keepalive to client");
                        if tx.send(Ok(Bytes::from_static(b":\n\n"))).await.is_err() {
                            return; // client disconnected
                        }
                    }
                }
            };

            match attempt_result {
                Err(e) if retries < MAX_RETRIES => {
                    retries += 1;
                    warn!(error = %e, retries, "pre-generation request send failed; retrying");
                    tokio::time::sleep(RETRY_BACKOFF).await;
                    continue 'retry;
                }
                other => break 'retry other,
            }
        };

        match upstream_response {
            Ok(resp) if resp.status().is_success() => {
                debug!(
                    status = resp.status().as_u16(),
                    "upstream accepted streaming request after slot-queue wait"
                );
                let outcome = stream_response_to_channel(
                    resp,
                    model_name_owned.clone(),
                    tags,
                    tx,
                    &connection,
                )
                .await;
                // Feed the terminal outcome to the watchdog: an empty
                // response is a strike, visible output resets the streak.
                //
                // Deliberately `saw_visible_output`, not "produced any frame":
                // a reasoning-only turn is a failed turn from the client's
                // point of view, and counting it as success reset this streak
                // on every retry, so the recycle watchdog never fired.
                upstream_health.record_stream_outcome(outcome.saw_visible_output);
                // Calibrate this model's chars-per-token ratio from the
                // real prompt-token count the upstream reported.
                if let Some(prompt_tokens) = outcome.prompt_tokens {
                    calibration.record(&model_name_owned, forwarded_chars, prompt_tokens);
                    // Prompt-cache telemetry. Recorded only alongside a real
                    // prompt-token count, so a request that never produced a
                    // usage frame is absent from the totals rather than
                    // counted as zero reuse.
                    cache_metrics.record(prompt_tokens, outcome.cached_tokens);
                }
                // KV cache save (opt-in): awaited, never detached, happens
                // after stream exhaustion and before the permit drops.
                if let (Some(cfg), Some(sid)) = (config.as_ref(), session_id.as_ref()) {
                    save_after_generation(cfg, sid).await;
                }
            }
            Ok(resp) => {
                let status = resp.status();
                let error_bytes = resp.bytes().await.unwrap_or_default();
                warn!(
                    status = status.as_u16(),
                    bytes = error_bytes.len(),
                    "upstream returned error during slot-queue wait"
                );
                // Preserve the upstream error's `type` and `code` so the
                // LLM Gateway extension (and VS Code) can identify errors
                // like `context_length_exceeded` rather than seeing an
                // opaque `server_error` wrapper.  Falls back to the
                // generic envelope only when the body is not valid JSON.
                // No panic paths: every operation is Option/Result-safe.
                let payload = match serde_json::from_slice::<serde_json::Value>(&error_bytes) {
                    Ok(upstream) => {
                        let msg = upstream
                            .pointer("/error/message")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("upstream returned an error");
                        let typ = upstream
                            .pointer("/error/type")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("server_error");
                        let code = upstream
                            .pointer("/error/code")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("upstream_error");
                        serde_json::json!({
                            "error": { "message": msg, "type": typ, "code": code }
                        })
                    }
                    Err(_) => {
                        // Non-JSON body — fall back to a generic wrapper
                        // that includes the raw bytes as context.
                        let body_str = String::from_utf8_lossy(&error_bytes);
                        serde_json::json!({
                            "error": {
                                "message": format!(
                                    "upstream returned {}: {}",
                                    status, body_str
                                ),
                                "type": "server_error",
                                "code": "upstream_error",
                            }
                        })
                    }
                };
                let frame = format!("data: {payload}\n\ndata: [DONE]\n\n");
                let human = payload
                    .pointer("/error/message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("upstream returned an error");
                let visible = visible_content_frame(
                    &model_name_owned,
                    &format!("⚠️ [proxy] upstream model server error ({status}): {human}"),
                );
                let frame = format!("{visible}{frame}");
                let _ = tx.send(Ok(Bytes::from(frame))).await;
            }
            Err(e) => {
                error!("upstream llama-server unreachable during slot-queue wait: {e}");
                let payload = serde_json::json!({
                    "error": {
                        "message": format!("upstream llama-server unavailable: {e}"),
                        "type": "server_error",
                        "code": "upstream_error",
                    }
                });
                let frame = format!("data: {payload}\n\ndata: [DONE]\n\n");
                let visible = visible_content_frame(
                    &model_name_owned,
                    &format!("⚠️ [proxy] upstream llama-server unavailable: {e}"),
                );
                let frame = format!("{visible}{frame}");
                let _ = tx.send(Ok(Bytes::from(frame))).await;
            }
        }
    });

    // Return 200 immediately — the client sees a live SSE stream right
    // away, keeps the connection open, and receives keepalive comments
    // while llama.cpp assigns a slot.
    let body = Body::from_stream(async_stream::stream! {
        let mut rx = rx;
        while let Some(item) = rx.recv().await {
            yield item;
        }
    });
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no")
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
