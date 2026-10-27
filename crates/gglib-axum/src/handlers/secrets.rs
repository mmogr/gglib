//! `POST /api/secrets/reveal`: a stored secret, for the page's Show and Copy,
//! and the proxy key its dashboard, tray and Clear Cache present (ADR 0027).

use axum::Json;
use axum::extract::State;
use gglib_core::secret::{RevealedSecret, SecretRef, SettingSecret};

use crate::error::HttpError;
use crate::state::AppState;

/// The secret the body names, or `null` when nothing is stored there.
///
/// Every secret but one: the pairing key is this device's credential for
/// another machine, and a client on the network holding the proxy key must
/// not be able to read it. Only the CLI prints it, on this machine.
pub(crate) async fn reveal(
    State(state): State<AppState>,
    Json(which): Json<SecretRef>,
) -> Result<Json<RevealedSecret>, HttpError> {
    let pairing = SettingSecret::RemotePairingKey;
    if which == (SecretRef::Setting { secret: pairing }) {
        return Err(HttpError::Forbidden(format!(
            "this device's pairing key is shown only in a terminal on this machine: `{}`",
            pairing.reveal_command()
        )));
    }
    let value = state.core.secrets().reveal(&which).await?;
    Ok(Json(RevealedSecret { value }))
}
