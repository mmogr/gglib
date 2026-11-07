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
//! behaviour well beyond the queueing case this addresses, and would risk
//! masking a genuinely dead upstream.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use chrono::Utc;
use gglib_core::ports::RetryObserver;
use gglib_core::retry::{RetryDecision, RetryPolicy, decide, jitter_unit};
use reqwest::{Client, Response};
use serde_json::Value;

use super::super::Door;
use super::classify::{Failure, classify};

/// POST `body` to `url`, retrying transient upstream failures per `policy`.
///
/// Returns the first successful response, with its body untouched and ready to
/// stream. `observer`, when present, is notified of each backoff and of the
/// final give-up so a waiting user can be told what is happening.
///
/// `door` is `Some` only when `url` is a gglib endpoint's (today the
/// tunnel's loopback port): it supplies the bearer that endpoint demands, and
/// it is the only thing here that knows whose endpoint refused — see
/// [`terminal`].
pub(crate) async fn send_with_retry(
    client: &Client,
    url: &str,
    door: Option<&Door>,
    body: &Value,
    send_timeout: Duration,
    policy: &RetryPolicy,
    observer: Option<&Arc<dyn RetryObserver>>,
) -> Result<Response> {
    let started = Instant::now();
    let mut attempt: u32 = 0;

    loop {
        attempt += 1;

        let response = send_once(
            client,
            url,
            door.and_then(|d| d.key.as_deref()),
            body,
            send_timeout,
        )
        .await?;

        // `classify` reads nothing from a successful body, so the stream
        // decoder still receives it whole.
        let failure = match classify(response, Utc::now()).await {
            Ok(response) => return Ok(response),
            Err(failure) => failure,
        };

        let Failure::Retryable {
            retry_after,
            reason,
            ..
        } = &failure
        else {
            return Err(terminal(&failure, door));
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

/// Turn a failure nothing will be retried after into the error the caller
/// reports.
///
/// The door gets first refusal on explaining it, because only this point
/// knows both halves: the classifier has the upstream's `code` but not who
/// sent it, and the surface that chose the upstream knows who but is long
/// past the request by the time an answer comes back. The classifier's own
/// rendering is the fallback, so every failure this cannot improve on reads
/// exactly as it did before.
///
/// Two conditions gate the swap and both are load-bearing. `invalid_api_key`
/// means a key was refused, and which sentence is right depends on whose
/// endpoint refused it, so the sentence may only win where a door was dialled
/// at all — that is the `Option`; a paired machine's door names the machine,
/// this machine's names none. And an endpoint can fail for reasons its key is
/// innocent of, so the code has to agree as well — that is
/// [`Door::refusal`]. The sentence carries no status and no upstream wording,
/// so the rendered reason is written to the log here rather than left to the
/// caller, which would otherwise never see what the upstream actually said.
fn terminal(failure: &Failure, door: Option<&Door>) -> anyhow::Error {
    let explained = door.and_then(|door| Some((door, door.refusal(failure.code())?)));
    match explained {
        Some((door, message)) => {
            // The name is not a credential — `gglib remote status` and the
            // CLI's pre-turn banner both print it — so a log reader gets the
            // same name the user is being shown.
            tracing::warn!(
                machine = door.name.as_deref().unwrap_or("this machine"),
                reason = %failure.reason(),
                "the endpoint refused the key it was dialled with"
            );
            anyhow!(message)
        }
        None => anyhow!("{}", failure.reason()),
    }
}

/// One POST, bounded by the send timeout.
///
/// The timeout covers TCP connect through response headers, which
/// llama-server sends once a slot takes the request, before prefill: the body
/// asks for `return_progress`.
async fn send_once(
    client: &Client,
    url: &str,
    bearer: Option<&str>,
    body: &Value,
    send_timeout: Duration,
) -> Result<Response> {
    let secs = send_timeout.as_secs();
    let mut request = client.post(url).json(body);
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    tokio::time::timeout(send_timeout, request.send())
        .await
        .map_err(|_| anyhow!("llama-server connection timed out after {secs}s"))?
        .map_err(|e| anyhow!("request to llama-server failed: {e}"))
}
