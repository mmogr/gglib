//! `gglib web` — open the daemon's dashboard.
//!
//! The web UI is served by the daemon itself (one process, one port), so
//! this command reduces to: make sure the daemon is up, print the URL and
//! hand it to the system's launcher, which opens it in the browser.
//! `--no-open` only prints. `--share-lan` is the exception — LAN exposure is
//! an explicit foreground decision, so it forwards to the same code path as
//! `gglib daemon run --share-lan`.
//!
//! The URL carries the daemon's token as a fragment, `#token=…`, which the
//! page keeps for its own calls and strips from the address bar. A fragment
//! never leaves the browser in a request, and the URL is never logged: it is
//! printed here and handed to the launcher, nowhere else. The daemon mints a
//! new token at every start, so a link outlives no restart.

use std::io::Write;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::daemon_client;
use crate::presentation::style;

/// Execute the `web` command.
pub(crate) async fn execute(ctx: &CliContext, share_lan: bool, no_open: bool) -> Result<()> {
    if share_lan {
        // Foreground, eyes-open LAN mode — identical to `daemon run --share-lan`.
        return super::daemon::run(true, Vec::new()).await;
    }

    daemon_client::ensure_daemon(daemon_client::auth::daemon_api_key(ctx).await).await?;

    let token = daemon_client::auth::daemon_token();
    let url = dashboard_url(&daemon_client::base_url(), token.as_deref());
    announce_and_open(
        &url,
        no_open,
        |u| open::that_detached(u),
        &mut std::io::stderr(),
    )?;
    Ok(())
}

/// Print the banner for `url` to `out`, then, unless `no_open`, hand it to
/// `launch`. A launcher that fails is reported in one line that leaves its
/// error out (the error names the command line, token included); the link
/// is already on screen, so the command still succeeds.
fn announce_and_open(
    url: &str,
    no_open: bool,
    launch: impl FnOnce(&str) -> std::io::Result<()>,
    out: &mut impl Write,
) -> std::io::Result<()> {
    style::print_info_banner("Web Dashboard", "\u{1f680}");
    writeln!(out, "  \u{1f310} Local:   {url}")?;
    writeln!(out, "  \u{1f4ca} Daemon:  gglib daemon status")?;
    writeln!(out)?;
    writeln!(
        out,
        "  The dashboard is served by the gglib daemon; it keeps running"
    )?;
    writeln!(
        out,
        "  after this command returns. `gglib daemon stop` shuts it down."
    )?;
    style::print_banner_close();
    if !no_open && launch(url).is_err() {
        writeln!(out, "{OPEN_FAILED}")?;
    }
    Ok(())
}

const OPEN_FAILED: &str = "Could not open a browser; open the link above yourself.";

/// The dashboard's address, carrying `token` as a fragment when there is one.
fn dashboard_url(base: &str, token: Option<&str>) -> String {
    token.map_or_else(|| base.to_owned(), |token| format!("{base}/#token={token}"))
}

#[cfg(test)]
mod tests {
    use super::{OPEN_FAILED, announce_and_open, dashboard_url};

    const URL: &str = "http://127.0.0.1:9887/#token=abc123";

    /// Run `announce_and_open` into a buffer; the printed text.
    fn run(no_open: bool, launch: impl FnOnce(&str) -> std::io::Result<()>) -> String {
        let mut out = Vec::new();
        announce_and_open(URL, no_open, launch, &mut out).expect("a Vec never fails a write");
        String::from_utf8(out).expect("utf-8")
    }

    #[test]
    fn the_url_carries_the_token_as_a_fragment() {
        assert_eq!(dashboard_url("http://127.0.0.1:9887", Some("abc123")), URL);
    }

    #[test]
    fn without_a_token_the_url_is_the_bare_address() {
        assert_eq!(
            dashboard_url("http://127.0.0.1:9887", None),
            "http://127.0.0.1:9887"
        );
    }

    #[test]
    fn the_launcher_gets_exactly_the_printed_url_once() {
        let mut calls = Vec::new();
        let printed = run(false, |u| {
            calls.push(u.to_owned());
            Ok(())
        });
        assert_eq!(calls, vec![URL.to_owned()]);
        assert_eq!(printed.matches(URL).count(), 1, "{printed}");
        assert!(!printed.contains(OPEN_FAILED), "{printed}");
    }

    #[test]
    fn no_open_never_calls_the_launcher() {
        let mut calls = 0;
        let printed = run(true, |_| {
            calls += 1;
            Ok(())
        });
        assert_eq!(calls, 0);
        assert_eq!(printed.matches(URL).count(), 1, "{printed}");
        assert!(!printed.contains(OPEN_FAILED), "{printed}");
    }

    #[test]
    fn a_failing_launcher_is_reported_without_its_error() {
        let mut calls = 0;
        // Shaped like the open crate's real error, which names the command line.
        let printed = run(false, |_| {
            calls += 1;
            Err(std::io::Error::other(format!(
                "Launcher \"/usr/bin/open\" \"--\" \"{URL}\" failed SENTINEL-abc123"
            )))
        });
        assert_eq!(calls, 1);
        assert!(printed.contains("Could not open a browser"), "{printed}");
        assert!(!printed.contains("SENTINEL"), "{printed}");
        assert_eq!(printed.matches(URL).count(), 1, "{printed}");
    }
}
