//! The paired machine, as the daemon reports it.
//!
//! A `#[path]` child of `target.rs`, split off when the decisions there
//! brought the file to the size budget. Everything here talks to the daemon
//! and reads the pairing; nothing here decides anything — the one decision,
//! that this is where a [`Target::Remote`](super::Target::Remote) upstream
//! comes from, is the parent's.

use anyhow::{Result, anyhow, bail};
use gglib_runtime::FarMachine;

use super::Upstream;
use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonProbe};
use crate::handlers::agent_chat::config::BannerInfo;
use crate::handlers::agent_chat::upstream;
use crate::presentation::style;

/// The machine on the other end of the tunnel, as the daemon reports it.
pub(super) async fn remote_upstream(ctx: &CliContext, banner: &BannerInfo) -> Result<Upstream> {
    let client = reqwest::Client::new();
    if !matches!(daemon_client::probe(&client).await, DaemonProbe::Running) {
        bail!(
            "--remote needs the daemon running and connected to the other machine: \
             `gglib remote connect` first"
        );
    }
    let handle = daemon_client::DaemonHandle {
        client,
        api_key: daemon_client::auth::daemon_api_key(ctx).await,
    };
    let status = handle.remote_status().await?;
    let Some(connection) = status.connected else {
        bail!("not connected to a remote machine — `gglib remote connect [<ticket>-<code>]` first");
    };
    let key = ctx
        .app
        .settings()
        .get()
        .await
        .map_err(|e| anyhow!("failed to load settings: {e}"))?
        .remote_pairing
        .map(|stored| stored.api_key)
        .ok_or_else(|| {
            anyhow!(
                "connected to a remote machine, but this one holds no key for it — pair again \
                 with the full `<ticket>-<code>` string from `gglib remote enable` there"
            )
        })?;

    if !banner.quiet {
        style::print_info_banner("Info", "\u{2139}\u{fe0f}");
        eprintln!(
            "  Asking the remote machine {} at {} ({})",
            connection.ticket_fingerprint, connection.base_url, connection.path
        );
        if let Some(ref s) = banner.sampling {
            upstream::print_sampling_lines(s);
        }
        style::print_banner_close();
    }

    Ok(Upstream {
        base_url: server_root(&connection.base_url),
        // The banner above has just named this machine to the user; the key
        // carries that name onward so a later refusal of it can name the same
        // machine, which by then nothing downstream could look up.
        far_machine: Some(FarMachine {
            key,
            fingerprint: connection.ticket_fingerprint,
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
