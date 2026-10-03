//! `gglib remote join` and `disconnect`: this machine as the laptop.
//!
//! Stopping the far machine is not here: it is `gglib daemon stop --remote`
//! (ADR 0013), beside the local stop.

use anyhow::Result;
use gglib_app_services::{RemoteJoinBody, RemoteJoinResponse, RemoteStatus};
use gglib_core::domain::UNNAMED_PAIRED;

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonProbe};

/// What `gglib remote join` was asked for.
#[derive(Debug, Clone, Default)]
pub(crate) struct JoinArgs {
    /// `<ticket>-<code>`, a bare ticket, or `None` for the last one.
    pub pairing: Option<String>,
    /// The loopback port to bind here.
    pub port: Option<u16>,
    /// A self-hosted relay URL for this side.
    pub relay: Option<String>,
    /// Dial only the paths the ticket carries.
    pub no_discovery: bool,
}

/// Execute `gglib remote join`.
pub(crate) async fn join(ctx: &CliContext, args: JoinArgs) -> Result<()> {
    let handle =
        daemon_client::ensure_daemon(daemon_client::auth::daemon_api_key(ctx).await).await?;

    let first_pairing = args
        .pairing
        .as_deref()
        .is_some_and(|p| p.rsplit_once('-').is_some_and(|(_, code)| code.len() == 6));
    eprintln!(
        "  Joining\u{2026} (reaching the other machine can take a few seconds{})",
        if first_pairing {
            ", then the code is redeemed"
        } else {
            ""
        }
    );
    let joined = handle
        .remote_join(&RemoteJoinBody {
            pairing: args.pairing,
            port: args.port,
            relay: args.relay,
            discovery: Some(!args.no_discovery),
        })
        .await?;

    eprintln!();
    for line in joined_lines(&joined) {
        eprintln!("{line}");
    }
    eprintln!();
    if let Some(wanted) = joined.moved_from {
        eprintln!(
            "  Port {wanted} was taken by something else, so this is on {} instead — and that \
             is the port remembered for next time.",
            joined.base_url
        );
    }
    eprintln!("  The other machine is now at:  {}", joined.base_url);
    eprintln!("  Any other OpenAI-compatible client pointed there needs this device's key as its");
    eprintln!("  API key, which this prints:");
    eprintln!("    gglib remote key --show");
    eprintln!("  gglib's own attach it themselves:");
    // The two halves do not rhyme, and cannot: `q`'s model is `-m`, `chat`'s
    // is the positional and has no short flag. Printed as `chat --remote -m`
    // it was a command that did not parse — `error: unexpected argument '-m'`.
    eprintln!("    gglib q --remote -m <model> \"\u{2026}\"     gglib chat --remote <model>");
    eprintln!("  The model is one the other machine serves; without a name the request 404s.");
    eprintln!();
    eprintln!("  Close it:  gglib remote disconnect");
    Ok(())
}

/// What `join` says it did: which machine it joined, whether it paired, and
/// which pairing it replaced, each machine by its name.
///
/// Settings keep one pairing, so a pairing with a second machine drops the
/// first one's key, and #1042 is that the screen says so. A function rather
/// than more `eprintln!`s so a test can read exactly what is printed.
fn joined_lines(joined: &RemoteJoinResponse) -> Vec<String> {
    let machine = joined.name.as_deref().unwrap_or(UNNAMED_PAIRED);
    let mut lines = vec![if joined.paired {
        format!(
            "  \u{2705} Joined {machine}, and paired with it. Its API key is stored here; next \
             time the ticket alone, or nothing, will do."
        )
    } else {
        format!("  \u{2705} Joined {machine}.")
    }];
    if let Some(earlier) = &joined.replaced {
        // Unnamed, both machines would be "the paired machine"; the one
        // dropped is told apart by when it was paired.
        let earlier = if earlier == UNNAMED_PAIRED {
            "the machine paired before"
        } else {
            earlier.as_str()
        };
        lines.push(format!(
            "  This replaces the pairing with {earlier}: this machine keeps one pairing, so \
             reaching it again takes a fresh `gglib remote invite` there."
        ));
    }
    lines
}

/// Execute `gglib remote disconnect`.
#[allow(
    clippy::single_match_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) async fn disconnect(ctx: &CliContext) -> Result<()> {
    let client = gglib_proxy::loopback::client();
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

/// The connect side's lines of `gglib remote status`.
pub(super) fn print_connection(status: &RemoteStatus) {
    eprintln!("{}", connection_line(status));
}

/// The connect side's line, naming the paired machine as every surface does:
/// by its name, never its fingerprint. A function so a test can read it.
fn connection_line(status: &RemoteStatus) -> String {
    let machine = status.paired_shown();
    let Some(c) = &status.connected else {
        return match (&status.stored_ticket_fingerprint, status.has_remote_key) {
            (Some(_), true) => {
                format!("  Connected: no \u{2014} `gglib remote join` dials {machine} again")
            }
            (Some(_), false) => format!(
                "  Connected: no \u{2014} last dialled {machine}, but no key is stored; pair again"
            ),
            (None, _) => "  Connected: no \u{2014} never paired with another machine".to_owned(),
        };
    };
    c.away_for_s.map_or_else(
        || format!("  Connected: {machine}  at {}  ({})", c.base_url, c.path),
        |secs| {
            format!(
                "  Connected: {machine}  at {}  \u{2014} away {}; the address stays, and it \
                 reconnects when that machine is back",
                c.base_url,
                for_how_long(secs)
            )
        },
    )
}

/// Seconds as a person reads them: `40s`, `3m`, `2h`. Also how `gglib model
/// list` says the paired machine is away.
pub(crate) fn for_how_long(secs: u64) -> String {
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
    /// The status line, and the line `gglib model list` ends with, are where
    /// this machine says how long the far one has been gone, and each is read
    /// at a glance: seconds while it could still be a blip, minutes for a lid
    /// that is closed, hours for a desktop that is off. Truncation, not
    /// rounding — "away 1m" at sixty-one seconds is the honest half of a
    /// figure that is about to change anyway.
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

#[cfg(test)]
#[path = "join_tests.rs"]
mod join_tests;
