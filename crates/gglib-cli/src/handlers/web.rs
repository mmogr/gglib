//! `gglib web` — open the daemon's dashboard.
//!
//! The web UI is served by the daemon itself (one process, one port), so
//! this command reduces to: make sure the daemon is up, print the URL.
//! `--share-lan` is the exception — LAN exposure is an explicit foreground
//! decision, so it forwards to the same code path as
//! `gglib daemon run --share-lan`.
//!
//! The URL carries the daemon's token as a fragment, `#token=…`, which the
//! page keeps for its own calls and strips from the address bar. A fragment
//! never leaves the browser in a request, and the URL is printed only here.
//! The daemon mints a new token at every start, so a link outlives no restart.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::daemon_client;
use crate::presentation::style;

/// Execute the `web` command.
pub(crate) async fn execute(ctx: &CliContext, share_lan: bool) -> Result<()> {
    if share_lan {
        // Foreground, eyes-open LAN mode — identical to `daemon run --share-lan`.
        return super::daemon::run(true, Vec::new()).await;
    }

    daemon_client::ensure_daemon(daemon_client::auth::daemon_api_key(ctx).await).await?;

    let token = daemon_client::auth::daemon_token();
    let url = dashboard_url(&daemon_client::base_url(), token.as_deref());
    style::print_info_banner("Web Dashboard", "\u{1f680}");
    eprintln!("  \u{1f310} Local:   {url}");
    eprintln!("  \u{1f4ca} Daemon:  gglib daemon status");
    eprintln!();
    eprintln!("  The dashboard is served by the gglib daemon; it keeps running");
    eprintln!("  after this command returns. `gglib daemon stop` shuts it down.");
    style::print_banner_close();
    Ok(())
}

/// The dashboard's address, carrying `token` as a fragment when there is one.
fn dashboard_url(base: &str, token: Option<&str>) -> String {
    token.map_or_else(|| base.to_owned(), |token| format!("{base}/#token={token}"))
}

#[cfg(test)]
mod tests {
    use super::dashboard_url;

    #[test]
    fn the_url_carries_the_token_as_a_fragment() {
        assert_eq!(
            dashboard_url("http://127.0.0.1:9887", Some("abc123")),
            "http://127.0.0.1:9887/#token=abc123"
        );
    }

    #[test]
    fn without_a_token_the_url_is_the_bare_address() {
        assert_eq!(
            dashboard_url("http://127.0.0.1:9887", None),
            "http://127.0.0.1:9887"
        );
    }
}
