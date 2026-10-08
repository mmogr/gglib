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
use crate::client_send::ClientSender;
use crate::connections::ConnectionGuard;
use crate::forward::{drain_events, failed_turn_body};
use crate::repair::{RepairContext, RepairTurn};
use crate::token_calibration::TokenCalibration;
use crate::upstream_health::UpstreamHealth;
use crate::upstream_read::{StreamBounds, first_byte_timeout_frame, upstream_events};
use gglib_core::cache_metrics::CacheMetricsStore;
use gglib_core::domain::DialectSpec;
use gglib_core::sse::SseEncoder;

/// Maximum number of retry attempts for the pre-generation connection phase
/// (TCP send / first-byte-deadline wait) before falling back to an inline
/// error frame. Total attempts = 1 (initial) + `MAX_RETRIES` = 3.
const MAX_RETRIES: u32 = 2;

/// Backoff between pre-generation retry attempts (100ms).
const RETRY_BACKOFF: std::time::Duration = std::time::Duration::from_millis(100);

/// Spawn the detached keepalive/streaming task and return the immediate SSE
/// response.
///
/// Bounds the pre-generation connection-establishment phase (TCP send /
/// first-byte-deadline wait, [`StreamBounds::first_byte`]) with a bounded
/// retry (`MAX_RETRIES` attempts, `RETRY_BACKOFF` apart). The retry after an
/// expired first-byte deadline is skipped while the watchdog has a recycle
/// pending; the retry after a failed send is not. Retries stop the moment a
/// response is obtained — once [`drain_events`] begins draining a successful
/// response, no further retries occur, and [`drain_events`] reports a
/// mid-stream failure, a silence past [`StreamBounds::idle`] among them.
///
/// While it waits for the headers it also returns as soon as `tx` closes, as
/// it does when the client closes its connection, or a keepalive outlasts
/// [`StreamBounds::send`]; returning drops the request.
///
/// `told` ([`crate::usage_reading`]) is passed through to [`drain_events`]:
/// `Some` forwards `prompt_progress` frames and adds the reading to the usage
/// frame; `None` sends an SSE comment for each and that frame unchanged.
///
/// When `config` and `session_id` are both `Some` (KV cache enabled), the KV
/// cache is saved via [`save_after_generation`] immediately after
/// [`drain_events`] returns — before the semaphore `permit` drops at the end
/// of this task — if
/// [`StreamOutcome::worth_saving`](crate::forward::StreamOutcome::worth_saving)
/// says so.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_and_return(
    req_builder: reqwest::RequestBuilder,
    body: Bytes,
    tx: ClientSender,
    rx: tokio::sync::mpsc::Receiver<Result<Bytes, std::io::Error>>,
    connection: ConnectionGuard,
    model_name_owned: String,
    dialect: Option<DialectSpec>,
    upstream_health: Arc<UpstreamHealth>,
    bounds: StreamBounds,
    calibration: Arc<TokenCalibration>,
    cache_metrics: Arc<CacheMetricsStore>,
    context_metrics: Arc<crate::metrics::ContextMetricsStore>,
    snapshot_seq: u64,
    forwarded_chars: Option<usize>,
    told: crate::usage_reading::Told,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
    config: Option<StreamConfig>,
    session_id: Option<String>,
    repair_turn: RepairTurn,
) -> Response {
    // `connection` is moved into this task so it lives exactly as long
    // as the streaming task does — dropped (unregistering from the
    // dashboard) whether the task finishes normally, the client
    // disconnects (the task is a detached `tokio::spawn`, but it returns
    // after it notices the response channel closed), or panics.
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
            let deadline = tokio::time::sleep(bounds.first_byte);
            tokio::pin!(deadline);

            let attempt_result = loop {
                tokio::select! {
                    biased;
                    // Returning drops `send_future`, and with it the request.
                    () = tx.closed() => return,
                    result = &mut send_future => break result,
                    () = &mut deadline => {
                        let deadline_secs = bounds.first_byte.as_secs();
                        // The single-slot upstream may legitimately be busy
                        // serving another (possibly minutes-long) request, in
                        // which case this request is correctly queued, not
                        // wedged. Only treat a deadline expiry as degradation
                        // when NO other connection is actively occupying the
                        // slot; otherwise extend the deadline and keep waiting.
                        if connection.others_active() {
                            warn!(
                                deadline_secs,
                                "slot-queue wait exceeded deadline but another request is active; extending (upstream busy, not wedged)"
                            );
                            deadline
                                .as_mut()
                                .reset(tokio::time::Instant::now() + bounds.first_byte);
                            continue;
                        }
                        // With a recycle pending the upstream is already known
                        // to be sick, and a retry would submit this request to
                        // it a second time while the first may still be queued
                        // there. Give up now, so the recycle is not held off.
                        if retries < MAX_RETRIES && !upstream_health.recycle_pending() {
                            retries += 1;
                            warn!(
                                deadline_secs,
                                retries,
                                "slot-queue wait exceeded first-byte deadline; retrying pre-generation phase"
                            );
                            tokio::time::sleep(RETRY_BACKOFF).await;
                            continue 'retry;
                        }
                        warn!(
                            deadline_secs,
                            "slot-queue wait exceeded first-byte deadline with no other active request; treating upstream as degraded"
                        );
                        upstream_health.record_timeout();
                        let frame = first_byte_timeout_frame(&model_name_owned, bounds.first_byte);
                        tx.send(Bytes::from(frame)).await;
                        return;
                    }
                    _ = keepalive_interval.tick() => {
                        debug!("slot-queue wait: sending SSE keepalive to client");
                        if !tx.send(Bytes::from_static(b":\n\n")).await {
                            return; // client disconnected or stopped reading
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
                // The repair re-issue needs the same endpoint, headers and
                // body the original went out with. `try_clone` succeeds
                // because the body is `Bytes` — the sibling call above
                // `expect`s on that same invariant.
                //
                // A `None` here is therefore a should-never-happen, and it is
                // worth saying so out loud: it silently disables repair *and*
                // the tool-call hold-back for the whole turn, so the client's
                // frames go out unwithheld and a malformed call reaches it
                // unaltered. Degrading is the right behaviour; degrading
                // without a trace is what made this hard to reason about.
                let cloned = req_builder.try_clone();
                if cloned.is_none() {
                    warn!(
                        model = %model_name_owned,
                        "request builder could not be cloned; tool-call repair and hold-back \
                         are disabled for this turn"
                    );
                }
                let repair = cloned.map(|builder| RepairContext {
                    req_builder: builder,
                    request_body: body.clone(),
                    turn: repair_turn,
                });
                let outcome = drain_events(
                    upstream_events(resp.bytes_stream(), bounds.idle),
                    model_name_owned.clone(),
                    dialect,
                    tx,
                    &connection,
                    repair,
                    told,
                )
                .await;
                if outcome.repair_attempted {
                    warn!(
                        model = %model_name_owned,
                        succeeded = outcome.repair_succeeded,
                        "tool-call repair attempted"
                    );
                    // Back-patched after the response streams, like the drift
                    // alarm's flag: the decision is only known once the turn
                    // has finished, by which time the snapshot already exists.
                    context_metrics.flag_tool_repair(snapshot_seq, outcome.repair_succeeded);
                }
                if outcome.upstream_errored {
                    warn!(
                        model = %model_name_owned,
                        "turn died on an upstream mid-stream failure"
                    );
                    // Back-patched like the repair flag: the fact is only
                    // known once the stream has ended, by which time the
                    // snapshot already exists.
                    context_metrics.flag_stream_error(snapshot_seq);
                }
                // The counted-only instruments. None of these change what the
                // client receives — they exist so the escalation ladder is
                // built against measured failure rates rather than guesses
                // about which failures are common.
                if outcome.finish_reason.as_deref() == Some("length") {
                    context_metrics.flag_truncated_generation(snapshot_seq);
                }
                // A client that left before the first generated token says
                // nothing about the model's answer, so it is not counted here.
                if !outcome.saw_visible_output && !outcome.left_before_first_token {
                    warn!(
                        model = %model_name_owned,
                        reasoning_only = outcome.saw_reasoning,
                        "turn produced no client-renderable output"
                    );
                    context_metrics.flag_empty_response(snapshot_seq, outcome.saw_reasoning);
                }
                if outcome.normalization_errored {
                    context_metrics.flag_normalization_error(snapshot_seq);
                }
                if outcome.unvalidatable_schema {
                    context_metrics.flag_unvalidatable_schema(snapshot_seq);
                }
                // Feed the terminal outcome to the watchdog. The precedence
                // between "produced output", "died upstream" and "the client
                // left" lives in `health_verdict`, so this stays one line and
                // the rule stays in one place.
                upstream_health.record_stream_outcome(outcome.health_verdict());
                // Drift alarm: dialect markup survived normalization into
                // client-visible output. Log it and back-patch this
                // request's dashboard snapshot.
                if let Some(marker) = &outcome.dialect_residue {
                    warn!(
                        model = %model_name_owned,
                        marker = %marker,
                        "dialect residue reached client-visible output"
                    );
                    context_metrics.flag_dialect_residue(snapshot_seq);
                }
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
                if outcome.worth_saving()
                    && let (Some(cfg), Some(sid)) = (config.as_ref(), session_id.as_ref())
                {
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
                let body = upstream_error_body(&model_name_owned, status, &error_bytes);
                tx.send(Bytes::from(body)).await;
            }
            Err(e) => {
                error!("upstream llama-server unreachable during slot-queue wait: {e}");
                let body = unreachable_body(&model_name_owned, &e);
                tx.send(Bytes::from(body)).await;
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

/// The body a streaming client is sent when the upstream answers with an
/// error `status`: a visible notice, the error frame, then `[DONE]`.
///
/// The frame keeps the upstream error's `type` and `code`, so the LLM Gateway
/// extension (and VS Code) can identify errors like `context_length_exceeded`
/// rather than seeing an opaque `server_error` wrapper. It falls back to the
/// generic envelope, around the raw bytes, only when `body` is not JSON.
fn upstream_error_body(model: &str, status: StatusCode, body: &[u8]) -> String {
    let (message, error) = match serde_json::from_slice::<serde_json::Value>(body) {
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
            let error = SseEncoder::upstream_error_frame(msg, typ, code);
            (msg.to_owned(), error)
        }
        Err(_) => {
            let raw = String::from_utf8_lossy(body);
            let msg = format!("upstream returned {status}: {raw}");
            let error = SseEncoder::upstream_error_frame(&msg, "server_error", "upstream_error");
            (msg, error)
        }
    };
    let notice = format!("⚠️ [proxy] upstream model server error ({status}): {message}");
    failed_turn_body(model, &notice, &error)
}

/// The body a streaming client is sent when the request could not be sent,
/// or no response came back, for the reason `error` gives: a visible notice,
/// the error frame, then `[DONE]`.
fn unreachable_body(model: &str, error: &impl std::fmt::Display) -> String {
    let message = format!("upstream llama-server unavailable: {error}");
    let frame = SseEncoder::upstream_error_frame(&message, "server_error", "upstream_error");
    failed_turn_body(model, &format!("⚠️ [proxy] {message}"), &frame)
}

#[cfg(test)]
#[path = "sse_stream_tests.rs"]
mod tests;
