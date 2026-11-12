//! The paired machine, as this machine reaches it.
//!
//! A `#[path]` child of `target.rs`. Everything here talks to the daemon and
//! reads the pairing. [`Paired`] is the daemon and its connection, which a
//! command that asks the daemon about the paired machine needs; [`Far`] is
//! the far proxy itself, as the `FarProxy` the daemon's own `/api/remote`
//! routes use, for a turn or a proxy client that talks to it directly. Each
//! is established once, so "connected" and "holding a key" are refused with
//! one set of sentences. Nothing here decides anything — which commands get
//! either at all is the parent's table.

use anyhow::{Result, anyhow, bail};
use gglib_app_services::{FarProxy, GuiError, RemoteConnection, far_credentials};

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonHandle};

/// This machine's daemon, connected to the paired machine: the handle a
/// command asks the daemon through, the connection it reported, and the
/// name that machine is shown by.
pub(crate) struct Paired {
    /// The daemon, ready to be asked.
    pub handle: DaemonHandle,
    /// The connect side as the daemon reported it.
    pub connection: RemoteConnection,
    /// What a sentence calls that machine: its name, never its fingerprint.
    pub name: String,
}

/// The paired machine, as this machine reaches it directly: its proxy
/// through the tunnel, with the key this machine received when it paired.
pub(crate) struct Far {
    /// The far proxy, the same client the daemon's `/api/remote` routes use.
    pub proxy: FarProxy,
}

impl super::Target {
    /// This machine's daemon, connected to the paired machine. Only
    /// [`Remote`](super::Target::Remote) has one; asking
    /// [`Local`](super::Target::Local) is a programming error and says so.
    ///
    /// # Errors
    ///
    /// A daemon that is not running or not connected, each in the sentence
    /// that names the command to run.
    pub(crate) async fn paired(self) -> Result<Paired> {
        if self == Self::Local {
            bail!("internal: the local target has no far machine");
        }
        let Ok(handle) = daemon_client::running().await else {
            bail!(
                "reaching the paired machine needs the daemon running and connected to it: \
                 `gglib remote join` first"
            );
        };
        let status = handle.remote_status().await?;
        let name = status.paired_shown().to_owned();
        let Some(connection) = status.connected else {
            bail!(
                "not connected to a remote machine — `gglib remote join [<ticket>-<code>]` first"
            );
        };
        Ok(Paired {
            handle,
            connection,
            name,
        })
    }

    /// The paired machine, ready to be asked directly, with the key this
    /// machine holds for it and no other (`far_credentials`).
    ///
    /// # Errors
    ///
    /// As [`paired`](Self::paired), and a pairing this machine holds no key
    /// for — in the sentence that names the command to run.
    pub(crate) async fn far(self, ctx: &CliContext) -> Result<Far> {
        let Paired { connection, .. } = self.paired().await?;
        let stored = ctx
            .app
            .settings()
            .get()
            .await
            .map_err(|e| anyhow!("failed to load settings: {e}"))?
            .remote_pairing;
        let refused = |e: GuiError| match e {
            GuiError::Conflict(message) => anyhow!(message),
            other => anyhow!(other),
        };
        let credentials =
            far_credentials(stored.as_ref(), &connection.ticket_fingerprint).map_err(refused)?;
        Ok(Far {
            proxy: FarProxy::new(&connection.base_url, &credentials).map_err(refused)?,
        })
    }
}
