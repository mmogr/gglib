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

use std::path::Path;
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
    /// The model that answered, by the Model ID a client sends for it when
    /// the answer names a library model's file (llama-server names a model
    /// by its file), else as the answer named it. A pinned endpoint refuses
    /// every other model, so a proof of one asks for the pin.
    pub answered_by: String,
    /// The id the request named.
    pub asked_for: String,
    /// How long the answer took, the load included.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub elapsed_ms: u64,
}

/// The daemon's error for a proof of model `gglib_id` that `refusal`
/// refused, the proof having been `asked` at that moment.
///
/// A model that did not load is refused as every failed load is (#1959 C5),
/// in the whole sentence: the endpoint tells its clients, the daemon's proof
/// among them, only the short form, so the sentence is read from `runtime`,
/// the one the endpoint loaded with, in this process. Only a failure of this
/// model, recorded since the proof was asked, is read: an earlier failure,
/// or another model's, is not why this proof failed, and the endpoint's own
/// short form stands. The whole sentence goes out only on the daemon's
/// `POST /api/proxy/prove`, which only the owner reaches.
pub(crate) fn refused(
    refusal: ProofRefusal,
    runtime: &dyn gglib_core::ports::ModelRuntimePort,
    gglib_id: i64,
    asked: Instant,
) -> crate::GuiError {
    if !refusal.load_failed {
        return crate::GuiError::ValidationFailed(refusal.reason);
    }
    let whole = u32::try_from(gglib_id)
        .ok()
        .and_then(|id| runtime.last_load_failure(id))
        .filter(|(at, _)| *at >= asked)
        .map(|(_, failure)| failure.sentence());
    crate::GuiError::LoadFailed(whole.unwrap_or(refusal.reason))
}

/// Why a proof did not get an answer, as a sentence every surface prints.
/// The route answers it as the daemon's error envelope, not as this shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofRefusal {
    pub reason: String,
    /// The model did not load (the endpoint's `model_load_failed`), and
    /// `reason` is its sentence as the endpoint gave it.
    pub load_failed: bool,
}

impl ProofRefusal {
    fn because(reason: String) -> Self {
        Self {
            reason,
            load_failed: false,
        }
    }

    /// The endpoint's refusal with `status` and `body`: a model that did not
    /// load is its sentence alone (#1959 C5), anything else the status and
    /// the body as they came.
    fn answered(status: reqwest::StatusCode, body: &str) -> Self {
        let error = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|v| v.get("error").cloned());
        let load_failed = error
            .as_ref()
            .filter(|e| {
                e.get("code").and_then(serde_json::Value::as_str)
                    == Some(gglib_core::ports::LoadFailure::CODE)
            })
            .and_then(|e| e.get("message").and_then(serde_json::Value::as_str));
        load_failed.map_or_else(
            || Self::because(format!("{status}: {}", body.trim())),
            |sentence| Self {
                reason: sentence.to_owned(),
                load_failed: true,
            },
        )
    }
}

/// Wait for the endpoint at `base_url` to answer `/health`, then send one
/// minimal completion for `model` and say who answered.
///
/// `key` is what the request carries as its bearer, when the endpoint
/// demands one; it is read, sent and forgotten. `library` is each library
/// model's file beside the Model ID a client sends for it
/// ([`library_files`]): the answer is said by Model ID ([`named_as_listed`]).
pub async fn prove(
    client: &reqwest::Client,
    base_url: &str,
    model: &str,
    key: Option<&str>,
    library: &[(std::path::PathBuf, String)],
    bind_timeout: Duration,
    load_timeout: Duration,
) -> Result<ProofOutcome, ProofRefusal> {
    let root = base_url.strip_suffix("/v1").unwrap_or(base_url);
    if !wait_for_bind(client, root, bind_timeout).await {
        return Err(ProofRefusal::because(format!(
            "the endpoint at {base_url} did not answer /health within {}s",
            bind_timeout.as_secs()
        )));
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
        .map_err(|e| ProofRefusal::because(e.to_string()))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(ProofRefusal::answered(status, &body));
    }
    let answered_by = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("model").and_then(|m| m.as_str()).map(str::to_owned))
        .unwrap_or_else(|| model.to_owned());
    Ok(ProofOutcome {
        answered_by: named_as_listed(&answered_by, model, library),
        asked_for: model.to_owned(),
        elapsed_ms: u64::try_from(begun.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

/// The answer's model as a client names it.
///
/// llama-server names the model in its answer by the file it was started
/// with, so a proof answered through the endpoint reads as a path. An answer
/// that is one of `library`'s files (each given with the Model ID a client
/// sends for it) is said as that Model ID, `asked` first when two library
/// models share the file; anything else is kept as it came.
#[must_use]
pub fn named_as_listed(
    answered_by: &str,
    asked: &str,
    library: &[(std::path::PathBuf, String)],
) -> String {
    let answered = Path::new(answered_by);
    let mut matching = library
        .iter()
        .filter(|(file, _)| file.as_path() == answered);
    let first = matching.next();
    let asked_too = first
        .into_iter()
        .chain(matching)
        .find(|(_, id)| id == asked);
    asked_too
        .or(first)
        .map_or_else(|| answered_by.to_owned(), |(_, id)| id.clone())
}

/// Each library model's file beside the Model ID a client sends for it, as
/// [`prove`] reads them.
pub(crate) fn library_files(models: &[gglib_core::Model]) -> Vec<(std::path::PathBuf, String)> {
    let ids = crate::models::openai_ids(models);
    models
        .iter()
        .filter_map(|m| ids.get(&m.id).map(|id| (m.file_path.clone(), id.clone())))
        .collect()
}

/// Poll `/health` until the listener answers, or `bind_timeout` passes. An
/// attempt gets at most 2 seconds and never more than is left, so the wait
/// ends at its limit wherever a refused connection is slow to say so.
async fn wait_for_bind(client: &reqwest::Client, root: &str, bind_timeout: Duration) -> bool {
    let deadline = Instant::now() + bind_timeout;
    let url = format!("{root}/health");
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        if client
            .get(&url)
            .timeout(left.min(Duration::from_secs(2)))
            .send()
            .await
            .is_ok()
        {
            return true;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        tokio::time::sleep(Duration::from_millis(100).min(left)).await;
    }
}

#[cfg(test)]
#[path = "proxy_prove_tests.rs"]
mod tests;
