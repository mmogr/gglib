//! `gglib remote join` and `disconnect`: this machine as the laptop.
//!
//! `connect` is the name `join` had, and still reaches this for one release.
//!
//! Stopping the far machine used to live here as `kill`; it is
//! `gglib daemon stop --remote` now (ADR 0013), beside the local stop.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonProbe, RemoteConnectBody, RemoteStatusDto};

/// What `gglib remote join` was asked for.
#[derive(Debug, Clone, Default)]
pub(crate) struct ConnectArgs {
    /// `<ticket>-<code>`, a bare ticket, or `None` for the last one.
    pub pairing: Option<String>,
    /// The loopback port to bind here.
    pub port: Option<u16>,
    /// A self-hosted relay URL for this side.
    pub relay: Option<String>,
    /// Dial only the paths the ticket carries.
    pub no_discovery: bool,
    /// Whether the person typed `connect` rather than `join`.
    ///
    /// Only the hint depends on it. Saying nothing when `join` was typed is
    /// the point: a deprecation notice on the command that replaced the
    /// deprecated one teaches the wrong thing.
    pub under_old_name: bool,
}

/// Execute `gglib remote join`.
pub(crate) async fn connect(ctx: &CliContext, args: ConnectArgs) -> Result<()> {
    let handle =
        daemon_client::ensure_daemon(daemon_client::auth::daemon_api_key(ctx).await).await?;

    if args.under_old_name {
        eprintln!("  note: `gglib remote join` is `gglib remote join` now.");
        eprintln!(
            "        The old name still works this release and does the same thing; \
             nothing about a pairing you already have changes."
        );
    }
    let first_pairing = args
        .pairing
        .as_deref()
        .is_some_and(|p| p.rsplit_once('-').is_some_and(|(_, code)| code.len() == 6));
    eprintln!(
        "  Connecting\u{2026} (reaching the other machine can take a few seconds{})",
        if first_pairing {
            ", then the code is redeemed"
        } else {
            ""
        }
    );
    let connected = handle
        .remote_connect(&RemoteConnectBody {
            pairing: args.pairing,
            port: args.port,
            relay: args.relay,
            discovery: Some(!args.no_discovery),
        })
        .await?;

    eprintln!();
    if connected.paired {
        eprintln!(
            "  \u{2705} Paired with {} and connected. Its API key is stored here; next time the \
             ticket alone, or nothing, will do.",
            connected.ticket_fingerprint
        );
    } else {
        eprintln!("  \u{2705} Connected to {}.", connected.ticket_fingerprint);
    }
    eprintln!();
    if let Some(wanted) = connected.moved_from {
        eprintln!(
            "  Port {wanted} was taken by something else, so this is on {} instead — and that \
             is the port remembered for next time.",
            connected.base_url
        );
    }
    eprintln!("  The other machine is now at:  {}", connected.base_url);
    eprintln!("  Any OpenAI-compatible client pointed there needs its API key; gglib's own do:");
    // The two halves do not rhyme, and cannot: `q`'s model is `-m`, `chat`'s
    // is the positional and has no short flag. Printed as `chat --remote -m`
    // it was a command that did not parse — `error: unexpected argument '-m'`.
    eprintln!("    gglib q --remote -m <model> \"\u{2026}\"     gglib chat --remote <model>");
    eprintln!("  The model is one the other machine serves; without a name the request 404s.");
    eprintln!();
    eprintln!("  Close it:  gglib remote disconnect");
    Ok(())
}

/// Execute `gglib remote disconnect`.
pub(crate) async fn disconnect(ctx: &CliContext) -> Result<()> {
    let client = reqwest::Client::new();
    match daemon_client::probe(&client).await {
        DaemonProbe::Running => {}
        _ => {
            eprintln!("  Daemon is not running \u{2014} nothing is connected.");
            return Ok(());
        }
    }
    let handle = daemon_client::DaemonHandle {
        client,
        api_key: daemon_client::auth::daemon_api_key(ctx).await,
    };
    let status = handle.remote_disconnect().await?;
    if status.connected.is_some() {
        anyhow::bail!("the daemon reported the connection still up after disconnect");
    }
    eprintln!("  Disconnected. The pairing is remembered; `gglib remote join` dials it again.");
    Ok(())
}

/// Execute `gglib remote kill`.
///
/// The connect side's lines of `gglib remote status`.
pub(super) fn print_connection(status: &RemoteStatusDto) {
    match &status.connected {
        Some(c) => match c.away_for_s {
            Some(secs) => eprintln!(
                "  Connected: {}  at {}  \u{2014} away {}; the address stays, and it reconnects when \
                 that machine is back",
                c.ticket_fingerprint,
                c.base_url,
                for_how_long(secs)
            ),
            None => eprintln!(
                "  Connected: {}  at {}  ({})",
                c.ticket_fingerprint, c.base_url, c.path
            ),
        },
        None => match (&status.stored_ticket_fingerprint, status.has_remote_key) {
            (Some(fp), true) => {
                eprintln!("  Connected: no \u{2014} `gglib remote join` dials {fp} again");
            }
            (Some(fp), false) => {
                eprintln!(
                    "  Connected: no \u{2014} last dialled {fp}, but no key is stored; pair again"
                );
            }
            (None, _) => eprintln!("  Connected: no \u{2014} never paired with another machine"),
        },
    }
}

/// Seconds as a person reads them: `40s`, `3m`, `2h`.
fn for_how_long(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s => format!("{}h", s / 3600),
    }
}

#[cfg(test)]
mod tests {
    use super::for_how_long;

    /// The away line reads in the unit a person would have used.
    ///
    /// The status line is the only place this machine says how long the far
    /// one has been gone, and it is read at a glance: seconds while it could
    /// still be a blip, minutes for a lid that is closed, hours for a desktop
    /// that is off. Truncation, not rounding — "away 1m" at sixty-one seconds
    /// is the honest half of a figure that is about to change anyway.
    #[test]
    fn how_long_a_machine_has_been_away_reads_in_the_unit_that_fits() {
        assert_eq!(for_how_long(0), "0s");
        assert_eq!(for_how_long(59), "59s");
        assert_eq!(for_how_long(60), "1m");
        assert_eq!(for_how_long(61), "1m");
        assert_eq!(for_how_long(3599), "59m");
        assert_eq!(for_how_long(3600), "1h");
        assert_eq!(
            for_how_long(86_400),
            "24h",
            "a day away is still hours, not days"
        );
    }
}
