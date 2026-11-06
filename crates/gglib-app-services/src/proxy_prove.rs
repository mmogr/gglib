//! The proof request: one real completion through the endpoint, so "the
//! endpoint works" is demonstrated rather than assumed.
//!
//! `gglib up` sent this itself. It belongs to the daemon, behind
//! `POST /api/proxy/prove`, so `gglib up`, `gglib proxy test` and the
//! Endpoint page's "Test it" send the same request and report the same
//! answer: who answered, in how long, or the reason it failed.
//!
//! The daemon talks to its own endpoint over HTTP like any other client, on
//! purpose: the point is to exercise the router, admission control, model
//! resolution and the forward pipeline. It authenticates with the key the
//! endpoint demands and never returns it.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::json;

/// How long to keep polling for the listener before giving up on it. The
/// supervisor says the proxy is running before its socket answers.
pub const BIND_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the first request may take.
///
/// Generous on purpose: this covers reading tens of gigabytes of weights off
/// disk on a cold page cache. Concurrent requests queue behind the same load
/// rather than being refused, so waiting is the correct behaviour.
pub const LOAD_TIMEOUT: Duration = Duration::from_mins(10);

/// What the proof found: the endpoint answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ProofOutcome {
    /// The model the answer names, which is the one that answered. A
    /// pinned endpoint refuses every other model, so a proof of one asks
    /// for the pin.
    pub answered_by: String,
    /// The id the request named.
    pub asked_for: String,
    /// How long the answer took, the load included.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub elapsed_ms: u64,
}

/// Why a proof did not get an answer, as a sentence every surface prints.
/// The route answers it as the daemon's error envelope, not as this shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofRefusal {
    pub reason: String,
}

/// Wait for the endpoint at `base_url` to answer `/health`, then send one
/// minimal completion for `model` and say who answered.
///
/// `key` is what the request carries as its bearer, when the endpoint
/// demands one; it is read, sent and forgotten.
pub async fn prove(
    client: &reqwest::Client,
    base_url: &str,
    model: &str,
    key: Option<&str>,
    bind_timeout: Duration,
    load_timeout: Duration,
) -> Result<ProofOutcome, ProofRefusal> {
    let root = base_url.strip_suffix("/v1").unwrap_or(base_url);
    if !wait_for_bind(client, root, bind_timeout).await {
        return Err(ProofRefusal {
            reason: format!(
                "the endpoint at {base_url} did not answer /health within {}s",
                bind_timeout.as_secs()
            ),
        });
    }
    let begun = Instant::now();
    let mut request = client
        .post(format!("{base_url}/chat/completions"))
        .timeout(load_timeout);
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    // One minimal completion: enough to force the model to load, short
    // enough that generation time is noise next to load time.
    let response = request
        .json(&json!({
            "model": model,
            "messages": [{ "role": "user", "content": "hi" }],
            "max_tokens": 1,
            "stream": false,
        }))
        .send()
        .await
        .map_err(|e| ProofRefusal {
            reason: e.to_string(),
        })?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(ProofRefusal {
            reason: format!("{status}: {}", body.trim()),
        });
    }
    let answered_by = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("model").and_then(|m| m.as_str()).map(str::to_owned))
        .unwrap_or_else(|| model.to_owned());
    Ok(ProofOutcome {
        answered_by,
        asked_for: model.to_owned(),
        elapsed_ms: u64::try_from(begun.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

/// Poll `/health` until the listener answers, or `bind_timeout` passes.
async fn wait_for_bind(client: &reqwest::Client, root: &str, bind_timeout: Duration) -> bool {
    let deadline = Instant::now() + bind_timeout;
    let url = format!("{root}/health");
    while Instant::now() < deadline {
        if client
            .get(&url)
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

#[cfg(test)]
#[path = "proxy_prove_tests.rs"]
mod tests;
