//! The two requests the connect side makes *through* the tunnel.
//!
//! Both go to the far proxy over the local listener, which is the point: the
//! pairing route and the shutdown route are the far machine's, and this side
//! reaches them the way any client would. Neither is retried — one is a
//! one-time code and the other is a one-way door.

use std::time::Duration;

use serde::Deserialize;
use tracing::info;

use super::first_contact::Reached;
use crate::error::GuiError;

/// How long either request may take end to end. Generous because a first
/// request may still be finishing the hole punch; bounded because a tunnel
/// that never answers is a failure to report, not to wait out.
const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Deserialize)]
struct Paired {
    api_key: String,
}

/// Trade the one-time code for the far machine's API key.
///
/// The code travels twice on purpose: as the bearer, so the tunnel edge's
/// one-time grant admits the request without the real token, and in the
/// body, so the far proxy's pairing route can check it against the code that
/// session minted. Every refusal there is the same flat `401`.
///
/// # Errors
///
/// `ValidationFailed` for the refusal — the code is wrong, expired, spent,
/// or the far side was not enabled with one — and `Unavailable` when the
/// tunnel did not carry the request at all.
pub(super) async fn redeem(
    _reached: &Reached,
    base_url: &str,
    code: &str,
) -> Result<String, GuiError> {
    let client = client()?;
    let response = client
        .post(format!("{base_url}/remote/pair"))
        .bearer_auth(code)
        .json(&serde_json::json!({ "code": code }))
        .send()
        .await
        .map_err(|e| {
            GuiError::Unavailable(format!("the pairing request did not get through: {e}"))
        })?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(GuiError::ValidationFailed(
            "the far machine refused the pairing code — it may have expired (two minutes), been \
             used already, or been burned by wrong attempts; run `gglib remote enable --invite` there \
             again"
                .to_owned(),
        ));
    }
    let status = response.status();
    if !status.is_success() {
        return Err(GuiError::Unavailable(format!(
            "the pairing request was answered with {status}"
        )));
    }
    let paired: Paired = response.json().await.map_err(|e| {
        GuiError::Internal(format!(
            "the pairing response was not what was expected: {e}"
        ))
    })?;
    info!("redeemed the pairing code; this machine now holds the remote's API key");
    Ok(paired.api_key)
}

/// Stop the far daemon: `POST /v1/proxy/shutdown` with the confirmation word
/// the route requires (ADR 0012, decision 7). A one-way door.
///
/// `fingerprint` names the machine being stopped, for the one answer where
/// which machine it was is the whole point: a refused key. It is the paired
/// ticket's fingerprint, the name `gglib remote status` and the connect
/// confirmation already print.
///
/// # Errors
///
/// `ValidationFailed` when the stored key is refused, `Conflict` when the
/// far proxy is not running under a daemon, `Unavailable` when the request
/// did not get through.
pub(super) async fn kill(base_url: &str, api_key: &str, fingerprint: &str) -> Result<(), GuiError> {
    let client = client()?;
    let response = client
        .post(format!("{base_url}/proxy/shutdown"))
        .bearer_auth(api_key)
        .json(&serde_json::json!({ "confirm": "shutdown" }))
        .send()
        .await
        .map_err(|e| {
            GuiError::Unavailable(format!("the shutdown request did not get through: {e}"))
        })?;
    match response.status() {
        s if s.is_success() => {
            info!("the remote daemon accepted the shutdown");
            Ok(())
        }
        // Deliberately not "its API key has changed": `connect` will dial a
        // bare ticket for a different machine while leaving an earlier
        // pairing's key in place, so the key can be refused by a machine
        // whose own key never moved. The narrower claim is true in both, and
        // is the same one the chat path's refusal makes.
        reqwest::StatusCode::UNAUTHORIZED => Err(GuiError::ValidationFailed(format!(
            "the remote machine {fingerprint} refused the stored key — it is not that machine's \
             current API key; pair again with a fresh `gglib remote enable --invite` there"
        ))),
        reqwest::StatusCode::CONFLICT => Err(GuiError::Conflict(
            "the far proxy is not running under a daemon, so there is nothing to stop from here"
                .to_owned(),
        )),
        s => Err(GuiError::Unavailable(format!(
            "the shutdown request was answered with {s}"
        ))),
    }
}

fn client() -> Result<reqwest::Client, GuiError> {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| GuiError::Internal(format!("could not build an HTTP client: {e}")))
}
