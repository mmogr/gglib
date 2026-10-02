//! The paired machine, as this machine reaches it.
//!
//! A `#[path]` child of `target.rs`, split off when the decisions there
//! brought the file to the size budget. Everything here talks to the daemon
//! and reads the pairing. [`Paired`] is the daemon and its connection, which
//! a command that asks the daemon about the paired machine needs; [`Far`] is
//! that plus the key, for a turn or a proxy client that talks to the far
//! proxy itself. Each is established once, so "connected" and "holding a key"
//! are refused with one set of sentences. Nothing here decides anything —
//! which commands get either at all is the parent's table.

use anyhow::{Result, anyhow, bail};
use gglib_app_services::{GuiError, RemoteConnection, far_credentials};
use gglib_runtime::FarMachine;

use super::Upstream;
use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonHandle, DaemonProbe};
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

/// The paired machine, as this machine can reach it directly: the tunnel's
/// loopback port, the key this machine received when it paired, and the
/// name it knows the machine by.
pub(crate) struct Far {
    /// `http://127.0.0.1:<port>/v1`, exactly as the daemon reports it.
    pub base_url: String,
    /// The tunnel's loopback port, for a command that takes host and port.
    pub port: u16,
    /// The key this machine received when it paired.
    pub key: String,
    /// What a sentence calls that machine: its name, never its fingerprint.
    pub name: String,
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
        let client = gglib_proxy::loopback::client();
        if !matches!(daemon_client::probe(&client).await, DaemonProbe::Running) {
            bail!(
                "--remote needs the daemon running and connected to the other machine: \
                 `gglib remote join` first"
            );
        }
        let handle = DaemonHandle {
            client,
            api_key: daemon_client::auth::daemon_api_key(ctx).await,
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
        let Paired {
            connection, name, ..
        } = self.paired(ctx).await?;
        let stored = ctx
            .app
            .settings()
            .get()
            .await
            .map_err(|e| anyhow!("failed to load settings: {e}"))?
            .remote_pairing;
        let key = far_credentials(stored.as_ref(), &connection.ticket_fingerprint)
            .map_err(|e| match e {
                GuiError::Conflict(message) => anyhow!(message),
                other => anyhow!(other),
            })?
            .key;
        let port = reqwest::Url::parse(&connection.base_url)
            .ok()
            .and_then(|url| url.port())
            .ok_or_else(|| {
                anyhow!(
                    "the daemon reported the remote at {}, which names no port",
                    connection.base_url
                )
            })?;
        Ok(Far {
            base_url: connection.base_url,
            port,
            key,
            name,
            path: connection.path,
        })
    }
}

/// The machine on the other end of the tunnel, as a turn's upstream.
pub(super) async fn remote_upstream(ctx: &CliContext, banner: &BannerInfo) -> Result<Upstream> {
    let far = super::Target::Remote.far(ctx).await?;
    if !banner.quiet {
        style::print_info_banner("Info", "\u{2139}\u{fe0f}");
        eprintln!("  Asking {} at {} ({})", far.name, far.base_url, far.path);
        if let Some(ref s) = banner.sampling {
            upstream::print_sampling_lines(s);
        }
        style::print_banner_close();
    }
    Ok(Upstream {
        base_url: server_root(&far.base_url),
        // The banner above has just named this machine to the user; the key
        // carries that name onward so a later refusal of it can name the same
        // machine, which by then nothing downstream could look up.
        far_machine: Some(FarMachine {
            key: far.key,
            name: far.name,
        }),
    })
}

/// `http://127.0.0.1:41234/v1` → `http://127.0.0.1:41234`.
///
/// The daemon reports the URL a client pastes, with the `/v1`; the adapter
/// builds `/v1/chat/completions` from the server root itself.
fn server_root(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_owned()
}

#[cfg(test)]
mod tests {
    use super::server_root;

    #[test]
    fn the_v1_suffix_the_daemon_reports_is_removed_once() {
        assert_eq!(
            server_root("http://127.0.0.1:41234/v1"),
            "http://127.0.0.1:41234"
        );
        assert_eq!(
            server_root("http://127.0.0.1:41234/v1/"),
            "http://127.0.0.1:41234"
        );
        assert_eq!(
            server_root("http://127.0.0.1:41234"),
            "http://127.0.0.1:41234"
        );
    }
}
