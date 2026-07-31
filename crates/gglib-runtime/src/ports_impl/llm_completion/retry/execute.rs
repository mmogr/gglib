//! The retry loop.
//!
//! # Why this is safe to retry
//!
//! Every attempt here ends before the response body is touched: [`classify`]
//! hands a successful response back unread, and the caller only then builds the
//! stream decoder from it. So a retry can never replay tokens the user has
//! already seen or re-trigger a tool call — the window closes the instant a 2xx
//! is returned. Nothing downstream of that point retries.
//!
//! Transport failures (refused connection, send timeout) stay terminal, exactly
//! as before this module existed. Widening retry to cover them would change
//! behaviour well beyond the contention case this addresses, and would risk
//! masking a genuinely dead upstream.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use chrono::Utc;
use gglib_core::ports::RetryObserver;
use gglib_core::retry::{RetryDecision, RetryPolicy, decide, jitter_unit};
use reqwest::{Client, Response};
use serde_json::Value;

use super::classify::{Failure, classify};

/// POST `body` to `url`, retrying transient upstream failures per `policy`.
///
/// Returns the first successful response, with its body untouched and ready to
/// stream. `observer`, when present, is notified of each backoff and of the
/// final give-up so a waiting user can be told what is happening.
pub(crate) async fn send_with_retry(
    client: &Client,
    url: &str,
    body: &Value,
    send_timeout: Duration,
    policy: &RetryPolicy,
    observer: Option<&Arc<dyn RetryObserver>>,
) -> Result<Response> {
    let started = Instant::now();
    let mut attempt: u32 = 0;

    loop {
        attempt += 1;

        let response = send_once(client, url, body, send_timeout).await?;

        // `classify` reads nothing from a successful body, so the stream
        // decoder still receives it whole.
        let failure = match classify(response, Utc::now()).await {
            Ok(response) => return Ok(response),
            Err(failure) => failure,
        };

        let Failure::Retryable {
            retry_after,
            reason,
        } = &failure
        else {
            return Err(anyhow!("{}", failure.reason()));
        };

        let elapsed = started.elapsed();
        match decide(policy, attempt, *retry_after, elapsed, jitter_unit()) {
            RetryDecision::Retry { after } => {
                tracing::warn!(
                    attempt,
                    delay_ms = after.as_millis(),
                    reason = %reason,
                    "upstream unavailable — backing off before retry"
                );
                if let Some(observer) = observer {
                    observer.on_retry(attempt, after, reason);
                }
                // A plain awaited sleep, deliberately: when the caller's future
                // is dropped (the GUI's abort signal, a cancelled CLI turn) this
                // is dropped with it. A detached task would outlive the request
                // it belongs to.
                tokio::time::sleep(after).await;
            }
            RetryDecision::GiveUp(give_up) => {
                tracing::warn!(
                    attempts = attempt,
                    elapsed_ms = elapsed.as_millis(),
                    reason = %reason,
                    give_up = give_up.as_str(),
                    "upstream still unavailable — giving up"
                );
                if let Some(observer) = observer {
                    observer.on_exhausted(attempt, elapsed, reason);
                }
                return Err(anyhow!(
                    "{reason} (gave up after {attempt} attempts, {}: {})",
                    format_args!("{:.1}s", elapsed.as_secs_f64()),
                    give_up.as_str()
                ));
            }
        }
    }
}

/// One POST, bounded by the send timeout.
///
/// The timeout covers TCP connect through response headers, which includes
/// prompt pre-fill because llama-server withholds headers until pre-fill ends.
async fn send_once(
    client: &Client,
    url: &str,
    body: &Value,
    send_timeout: Duration,
) -> Result<Response> {
    let secs = send_timeout.as_secs();
    tokio::time::timeout(send_timeout, client.post(url).json(body).send())
        .await
        .map_err(|_| anyhow!("llama-server connection timed out after {secs}s"))?
        .map_err(|e| anyhow!("request to llama-server failed: {e}"))
}
