//! `gglib web` — open the daemon's dashboard.
//!
//! The web UI is served by the daemon itself (one process, one port), so
//! this command reduces to: make sure the daemon is up, print the URL.
//! `--share-lan` is the exception — LAN exposure is an explicit foreground
//! decision, so it forwards to the same code path as
//! `gglib daemon run --share-lan`.

use anyhow::Result;

use crate::daemon_client;
use crate::presentation::style;

/// Execute the `web` command.
pub(crate) async fn execute(share_lan: bool) -> Result<()> {
    if share_lan {
        // Foreground, eyes-open LAN mode — identical to `daemon run --share-lan`.
        return super::daemon::run(true, Vec::new()).await;
    }

    daemon_client::ensure_daemon().await?;

    let url = daemon_client::base_url();
    style::print_info_banner("Web Dashboard", "\u{1f680}");
    eprintln!("  \u{1f310} Local:   {url}");
    eprintln!("  \u{1f4ca} Daemon:  gglib daemon status");
    eprintln!();
    eprintln!("  The dashboard is served by the gglib daemon; it keeps running");
    eprintln!("  after this command returns. `gglib daemon stop` shuts it down.");
    style::print_banner_close();
    Ok(())
}
