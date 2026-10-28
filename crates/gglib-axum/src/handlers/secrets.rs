//! `POST /api/secrets/reveal`: a stored secret, for the page's Show and Copy,
//! and the proxy key its dashboard, tray and Clear Cache present (ADR 0027).
//!
//! A shared daemon's API key, which clients on the network hold, gets back
//! that key and no other secret: the page opened over the network holds only
//! the key, and presents it to the proxy for its dashboard and Clear Cache.
//! The key still opens every other `/api` route, through which it can run an
//! MCP server's command as the owner, so this closes the direct readback of
//! a stored value to the network and no more.

use axum::extract::State;
use axum::{Extension, Json};
use gglib_core::secret::{RevealedSecret, SecretRef, SettingSecret};

use crate::error::HttpError;
use crate::state::AppState;
use crate::trust::Admitted;

/// The secret the body names, or `null` when nothing is stored there.
///
/// To the daemon token, every secret but one: the pairing key is this
/// device's credential for another machine, which the page has no use for.
/// Only the CLI prints it, on this machine. To a shared daemon's API key,
/// the proxy key alone, answered with the key the request presented, so it
/// hands back nothing the caller did not send.
pub(crate) async fn reveal(
    State(state): State<AppState>,
    Extension(admitted): Extension<Admitted>,
    Json(which): Json<SecretRef>,
) -> Result<Json<RevealedSecret>, HttpError> {
    if let Admitted::ApiKey(key) = admitted {
        if which
            == (SecretRef::Setting {
                secret: SettingSecret::ProxyApiKey,
            })
        {
            return Ok(Json(RevealedSecret {
                value: Some(key.to_string()),
            }));
        }
        return Err(HttpError::Forbidden(
            "a shared daemon's API key reveals only itself. Any other stored secret is \
             shown only to the daemon's own token, from the machine the daemon runs on: \
             open the link `gglib web` prints there"
                .to_owned(),
        ));
    }
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
