//! The non-streaming half of `/v1/chat/completions`: one request up, one
//! body back, read whole, normalised, judged and answered.
//!
//! Its own module because [`crate::forward`] sits at its complexity-ratchet
//! ceiling, and the two paths share nothing past the request shaping: the
//! streaming path drains frames through a channel, this one buffers a body.
//! The shared reading of that body lives in [`crate::unary_body`], which the
//! embeddings route uses too, and the shared reading of a repair's answer in
//! [`crate::repair::read_second_draw`].
//!
//! # Repair on this path
//!
//! A buffered body needs none of the streaming path's hold-back: the whole
//! answer is in hand before anything is sent, so it is judged once read and,
//! on a violation, drawn again the way the streaming path draws again. What
//! this path cannot do is keep the wire warm while the second draw runs, so a
//! client whose own deadline is shorter than two generations gives up on a
//! repaired turn it would have accepted unrepaired. `docs/tool-call-repair.md`
//! says so under "Cost".

use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use tracing::{debug, error, warn};

use gglib_core::cache_metrics::CacheMetricsStore;
use gglib_core::domain::DialectSpec;

use crate::forward::{ForwardError, REPAIR_REISSUE_TIMEOUT};
use crate::metrics::ContextMetricsStore;
use crate::models::ErrorResponse;
use crate::repair::{Decision, RepairTurn, Skipped, choose, decide, read_second_draw};
use crate::unary_body::{answer_with, read_non_streaming_body, unreadable_upstream};

/// Send one shaped, non-streaming chat completion upstream and answer with
/// its body, drawn again when its tool call did not validate.
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
    turn: RepairTurn,
) -> Result<Response, ForwardError> {
    // A re-issue needs the same endpoint, headers and body the original goes
    // out with. Cloned before the body is attached, because `send` consumes
    // the builder. Nothing streams into the request, so the clone cannot
    // fail; a `None` only means the turn falls open to its first answer.
    let again = req_builder.try_clone();
    let response = match req_builder.body(body.clone()).send().await {
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

    // Read the full response and run it through the same dialect
    // normalization the streaming path applies, then judge what it says.
    let read = read_non_streaming_body(
        response,
        cache_metrics,
        dialect,
        Some((metrics, snapshot_seq)),
    );
    let (content_type, answer) = match read.await {
        Ok(read) => read,
        Err(e) => return Ok(unreadable_upstream(&e)),
    };
    let answer = repair(again, &body, answer, dialect, metrics, snapshot_seq, turn).await;
    Ok(answer_with(content_type, answer))
}

/// Judge `answer`, and draw again when its tool call does not validate.
///
/// Returns the body to answer with: the second draw when it validates, and
/// `answer` on every other path, so a repair can slow a turn down but never
/// degrade it — the rule the streaming path follows. Records what that path
/// records, so the dashboard's repair rate and the per-model ledger count
/// this path too.
async fn repair(
    again: Option<reqwest::RequestBuilder>,
    request: &Bytes,
    answer: Bytes,
    dialect: Option<&DialectSpec>,
    metrics: &ContextMetricsStore,
    snapshot_seq: u64,
    turn: RepairTurn,
) -> Bytes {
    let decision = decide(request, &answer, turn);
    // Recorded before the early return, for the reason the streaming path
    // gives: a client whose tools cannot be judged gets zero repair coverage,
    // and without this, zero evidence of it.
    if matches!(decision, Decision::Forward(Skipped::Unvalidatable)) {
        metrics.flag_unvalidatable_schema(snapshot_seq);
    }
    let Decision::Reissue { body, violations } = decision else {
        return answer;
    };
    warn!(
        violations = ?violations,
        "tool call does not match the advertised schema; asking upstream again"
    );
    let (chosen, did_repair) = match again {
        None => (answer, false),
        Some(again) => {
            let sent = again
                .timeout(REPAIR_REISSUE_TIMEOUT)
                .body(body)
                .send()
                .await;
            match read_second_draw(sent, dialect).await {
                // `choose` re-validates: a draw that is still wrong is discarded.
                Some(repaired) => choose(request, answer, repaired),
                None => (answer, false),
            }
        }
    };
    // Logged and counted once per attempt, as the streaming path does, so a
    // repaired `stream: false` turn leaves the same trace in a live run.
    warn!(succeeded = did_repair, "tool-call repair attempted");
    metrics.flag_tool_repair(snapshot_seq, did_repair);
    chosen
}

/// Non-streaming turns repaired end to end.
#[cfg(test)]
#[path = "forward_unary_repair_tests.rs"]
mod forward_unary_repair_tests;
