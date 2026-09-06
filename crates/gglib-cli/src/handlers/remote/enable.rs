//! `gglib remote enable`.

use std::io::IsTerminal as _;

use anyhow::Result;

use super::pairing_tui::{self, Outcome};
use crate::daemon_client::{self, RemoteEnableBody, RemoteEnableDto};

/// What `gglib remote enable` was asked for.
#[derive(Debug, Clone, Default)]
pub(crate) struct EnableArgs {
    /// Let tunnelled requests reach `/mcp`.
    pub allow_mcp: bool,
    /// A self-hosted relay URL.
    pub relay: Option<String>,
    /// Do not publish to or resolve through n0's discovery service.
    pub no_discovery: bool,
    /// Print the pairing string as text; no QR, no alternate screen.
    pub no_qr: bool,
}

/// Execute `gglib remote enable`.
///
/// Ensures the daemon is running, asks it to bring the tunnel up, and shows
/// the pairing once — in the alternate screen when stdout is a terminal, as
/// plain text otherwise.
pub(crate) async fn enable(args: EnableArgs) -> Result<()> {
    let handle = daemon_client::ensure_daemon().await?;

    if args.no_discovery {
        eprintln!(
            "  note: --no-discovery means this ticket carries only the paths it was minted with; \
             it stops working if this machine changes network."
        );
    }
    eprintln!("  Enabling remote access\u{2026} (finding a relay can take a few seconds)");
    let enabled = handle
        .remote_enable(&RemoteEnableBody {
            allow_mcp: args.allow_mcp,
            relay: args.relay,
            discovery: Some(!args.no_discovery),
        })
        .await?;

    if args.no_qr || !std::io::stdout().is_terminal() {
        print_plain(&enabled);
        print_notice(args.allow_mcp);
        return Ok(());
    }

    match pairing_tui::run(&handle, &enabled).await? {
        Outcome::Paired { peer } => {
            eprintln!();
            match peer {
                Some(peer) => eprintln!("  \u{2705} Paired with device {peer}."),
                None => eprintln!("  \u{2705} A device paired."),
            }
            eprintln!(
                "  It holds the API key now; the tunnel stays up until `gglib remote disable`."
            );
        }
        Outcome::Expired => {
            eprintln!();
            eprintln!(
                "  The pairing code expired and nobody paired. The tunnel is up; the ticket is \
                 still valid for a device that already holds the key."
            );
            eprintln!("  Run `gglib remote disable` then `gglib remote enable` for a fresh code.");
        }
        Outcome::Interrupted => {
            eprintln!();
            eprintln!(
                "  Left the pairing screen. The tunnel is still up; `gglib remote disable` stops it."
            );
        }
    }
    print_notice(args.allow_mcp);
    Ok(())
}

/// The pairing as plain text: for scripts, pipes, and terminals that cannot
/// draw. Everything printed here is a credential for two minutes.
fn print_plain(enabled: &RemoteEnableDto) {
    println!("pairing: {}", enabled.pairing);
    println!("ticket:  {}", enabled.ticket);
    println!("code:    {}", enabled.code);
    eprintln!();
    eprintln!("  On the other machine, within {}s:", enabled.expires_in_s);
    eprintln!("    gglib remote connect {}", enabled.pairing);
}

/// What enabling changed on *this* machine, said every time.
fn print_notice(allow_mcp: bool) {
    eprintln!();
    eprintln!(
        "  Remote access is on. The local proxy on 127.0.0.1 now requires the API key too \u{2014} \
         gglib's own clients read it from settings; a hand-configured client needs it added once."
    );
    if allow_mcp {
        eprintln!("  /mcp is reachable through the tunnel (--allow-mcp).");
    } else {
        eprintln!("  /mcp is not reachable through the tunnel; pass --allow-mcp to change that.");
    }
    eprintln!("  Stop broadcasting:  gglib remote disable");
}
