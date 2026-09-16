//! The one request the connect side makes *through* the tunnel.
//!
//! It goes to the far proxy over the local listener, which is the point: the
//! shutdown route is the far machine's, and this side reaches it the way any
//! client would. It is not retried, because it is a one-way door. Pairing,
//! the other request that used to be made from here, is modelpipe's now: the
//! far edge answers it, and `connect_open` calls `modelpipe::pair`.

use std::time::Duration;

use tracing::info;

use crate::error::GuiError;

/// How long the request may take end to end. Generous because a first
/// request may still be finishing the hole punch; bounded because a tunnel
/// that never answers is a failure to report, not to wait out.
const TIMEOUT: Duration = Duration::from_secs(20);

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
            "the remote machine {fingerprint} is not admitting this device's key — it has \
             either stopped trusting this device, or a key rotation there is still reaching the \
             tunnel, which clears itself within a few seconds. If waiting does not fix it, pair \
             again with a fresh `gglib remote invite` there"
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
    gglib_proxy::loopback::client_builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| GuiError::Internal(format!("could not build an HTTP client: {e}")))
}
