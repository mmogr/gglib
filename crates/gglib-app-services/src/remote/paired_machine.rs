//! The machine this one is paired with, as a request to it and a surface
//! showing it need it: [`far_credentials`], the check its key passes on every
//! request to that machine, and the name it is shown by.
//!
//! The name is that machine's own `machine_name`, read from its `/v1/models`
//! once each connect has come up. Not sooner: the read goes through the live
//! connection, which does not exist until `dial` installs it. It is kept on
//! the stored pairing, so every surface shows it from the record without
//! asking that machine anything, and it is refreshed on every connect. It is
//! that machine's account of itself and so untrusted: [`machine_name`] is
//! applied again here, whatever the far side applied. The fingerprint stays
//! the identity; the name is shown and never compared.

use gglib_core::RemotePairing;
use gglib_core::domain::machine_name;
use gglib_core::services::AppCore;
use tracing::debug;

use super::RemoteOps;
use super::far_proxy::{FarError, FarProxy};
use super::stored_pairing::{fingerprint, remember};
use crate::error::GuiError;

/// What this machine may send to the machine it is connected to.
///
/// The key it holds for that machine, that machine's fingerprint, and the
/// name it goes by. No `Debug`, so the key cannot be formatted into a log line
/// by accident.
pub struct FarCredentials {
    /// The key that machine issued this one when they paired.
    pub key: String,
    /// The ticket fingerprint of the machine connected to: its identity,
    /// never shown.
    pub fingerprint: String,
    /// The name the stored pairing has for it, when it has one.
    pub name: Option<String>,
}

/// The one check every request to the far machine passes: the stored key,
/// only when the stored pairing names the machine connected to.
///
/// `join` keeps the two in agreement, by refusing a bare ticket for a machine
/// this one holds no key for. This checks it again where the key leaves, so a
/// key one machine issued is never shown to another (#1042), whichever
/// surface is sending it.
///
/// # Errors
///
/// `Conflict` when nothing is stored, or what is stored is another machine's
/// pairing: both are what a fresh pairing fixes, and the message says so.
pub fn far_credentials(
    stored: Option<&RemotePairing>,
    connected_fingerprint: &str,
) -> Result<FarCredentials, GuiError> {
    stored
        .filter(|stored| fingerprint(stored).as_deref() == Some(connected_fingerprint))
        .map(|stored| FarCredentials {
            key: stored.api_key.clone(),
            fingerprint: connected_fingerprint.to_owned(),
            name: stored.name.clone(),
        })
        .ok_or_else(|| {
            GuiError::Conflict(
                "connected to a machine this one holds no key for — pair again with the full \
                 `<ticket>-<code>` string from `gglib remote invite` there, then \
                 `gglib remote join` with it"
                    .to_owned(),
            )
        })
}

impl RemoteOps {
    /// The name the machine just connected to goes by: read from it, kept on
    /// the stored pairing, and handed back as the record then holds it, for
    /// `join` to answer with.
    ///
    /// Never a reason for the join to fail. The read is one request, bounded
    /// by [`FarProxy::models`]' three seconds; a machine that does not answer
    /// in time, refuses, or gives no name keeps the name it had, and the next
    /// connect tries again. Each of those is logged at debug and nothing
    /// more, because the connection it happened on is up.
    pub(super) async fn learn_name(&self, connected: &str) -> Option<String> {
        learn_name_through(&self.core, self.far().await, connected).await
    }
}

/// [`RemoteOps::learn_name`], through `far`: the one part of it that needs
/// the live connection, so it is handed in rather than looked up.
pub(super) async fn learn_name_through(
    core: &AppCore,
    far: Result<FarProxy, GuiError>,
    connected: &str,
) -> Option<String> {
    let read = match far {
        Ok(far) => far.models().await.map(|listed| listed.machine_name),
        Err(e) => Err(FarError::Failed(e)),
    };
    match read {
        Ok(Some(raw)) => {
            if let Err(e) = keep_name(core, connected, &raw).await {
                debug!(error = %e, "could not keep the paired machine's name");
            }
        }
        Ok(None) => debug!("the paired machine gave no name"),
        Err(e) => debug!(error = ?e, "could not read the paired machine's name"),
    }
    let stored = core.settings().get().await.ok()?.remote_pairing?;
    (fingerprint(&stored).as_deref() == Some(connected))
        .then_some(stored.name)
        .flatten()
}

/// Keep `raw`, as [`machine_name`] reads it, as the name of the machine
/// `connected` names.
///
/// Only while the stored pairing still names that machine: a pairing with
/// another one, made while the read was under way, is not given this one's
/// name, and a pairing forgotten meanwhile stays forgotten. A name
/// [`machine_name`] does not keep is dropped rather than stored, and a name
/// the record already holds is not written again, so a connect to a machine
/// that has not been renamed writes nothing.
///
/// # Errors
///
/// `Internal` when settings cannot be read, and whatever [`remember`] says
/// of the write.
pub(super) async fn keep_name(core: &AppCore, connected: &str, raw: &str) -> Result<(), GuiError> {
    let Some(name) = machine_name(raw) else {
        return Ok(());
    };
    let names = |stored: &RemotePairing| fingerprint(stored).as_deref() == Some(connected);
    let held = core
        .settings()
        .get()
        .await
        .map_err(|e| GuiError::from(e).context("could not read settings"))?
        .remote_pairing;
    if held
        .as_ref()
        .is_none_or(|held| !names(held) || held.name.as_deref() == Some(name.as_str()))
    {
        return Ok(());
    }
    remember(core, |stored| {
        if let Some(stored) = stored.as_mut().filter(|stored| names(stored)) {
            stored.name = Some(name.clone());
        }
    })
    .await
}

#[cfg(test)]
#[path = "paired_machine_tests.rs"]
mod paired_machine_tests;
