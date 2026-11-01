//! The paired machine, as this machine reaches it.
//!
//! A `#[path]` child of `target.rs`, split off when the decisions there
//! brought the file to the size budget. Everything here talks to the daemon
//! and reads the pairing. [`Paired`] is the daemon and its connection, which
//! a command that asks the daemon about the paired machine needs; [`Far`] is
//! the far proxy itself, as the `FarProxy` the daemon's own `/api/remote`
//! routes use, for a turn or a proxy client that talks to it directly. Each
//! is established once, so "connected" and "holding a key"
//! are refused with one set of sentences. Nothing here decides anything —
//! which commands get either at all is the parent's table.

use anyhow::{Result, anyhow, bail};
use gglib_app_services::{FarProxy, GuiError, RemoteConnection, far_credentials};

use super::{TurnModel, Upstream};
use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonHandle};
use crate::handlers::agent_chat::config::BannerInfo;
use crate::handlers::agent_chat::upstream;
use crate::presentation::style;

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
/// through the tunnel, with the key this machine received when it paired,
/// and how the tunnel is reaching it.
pub(crate) struct Far {
    /// The far proxy, the same client the daemon's `/api/remote` routes use.
    pub proxy: FarProxy,
    /// How the far side is being reached: `direct`, `relayed`, `idle`.
    pub path: String,
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
    pub(crate) async fn paired(self, ctx: &CliContext) -> Result<Paired> {
        if self == Self::Local {
            bail!("internal: the local target has no far machine");
        }
        let Ok(handle) = daemon_client::running(ctx).await else {
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
        let Paired { connection, .. } = self.paired(ctx).await?;
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
            path: connection.path,
        })
    }
}

/// The machine on the other end of the tunnel, as a turn's upstream. The
/// banner names the model as that machine resolved it, `turn`.
pub(super) async fn remote_upstream(
    ctx: &CliContext,
    turn: Option<&TurnModel>,
    banner: &BannerInfo,
) -> Result<Upstream> {
    let Far { proxy, path } = super::Target::Remote.far(ctx).await?;
    if !banner.quiet {
        style::print_info_banner("Info", "\u{2139}\u{fe0f}");
        let asked = turn.map_or_else(|| proxy.shown_name().to_owned(), TurnModel::shown);
        eprintln!("  Asking {asked} at {} ({path})", proxy.base_url());
        if let Some(ref s) = banner.sampling {
            upstream::print_sampling_lines(s);
        }
        style::print_banner_close();
    }
    Ok(Upstream {
        base_url: proxy.server_root(),
        // The banner above has just named this machine to the user; the key
        // carries that name onward so a later refusal of it can name the same
        // machine, which by then nothing downstream could look up.
        far_machine: Some(proxy.far_machine()),
    })
}
