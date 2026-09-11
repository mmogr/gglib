//! `gglib remote invite`: pair one more device with this machine.

use std::io::IsTerminal as _;

use anyhow::Result;

use super::enable::{Arming, print_notice};
use super::pairing_tui::{self, Outcome};
use crate::bootstrap::CliContext;
use crate::daemon_client::{self, RemoteEnableDto};

/// Execute `gglib remote invite`.
///
/// The tunnel has to be up already; the daemon refuses otherwise and names
/// both commands that fix it. Nothing about the session changes — the flags
/// it was enabled with, the ticket, and every device already using it are
/// left exactly as they were.
///
/// Shares `enable --invite`'s screen rather than growing one of its own:
/// what a person is shown is the same thing, and the only reason `enable`
/// can show it is that it happens to be offering a code at the time.
pub(crate) async fn invite(ctx: &CliContext, no_qr: bool) -> Result<()> {
    let handle =
        daemon_client::ensure_daemon(daemon_client::auth::daemon_api_key(ctx).await).await?;

    let offered = handle.remote_invite().await?;

    if no_qr || !std::io::stdout().is_terminal() {
        print_plain(&offered);
        print_notice(offered.mcp_allowed, Arming::Invite);
        return Ok(());
    }

    match pairing_tui::run(&handle, &offered).await? {
        Outcome::Paired { peer } => {
            eprintln!();
            match peer {
                Some(peer) => eprintln!("  \u{2705} Paired with device {peer}."),
                None => eprintln!("  \u{2705} A device paired."),
            }
            eprintln!(
                "  It holds a key of its own now. `gglib remote list` shows it, and \
                 `gglib remote forget` retires that one device and no other."
            );
        }
        Outcome::Expired => {
            eprintln!();
            eprintln!(
                "  The pairing code expired and nobody paired. The tunnel is up and every \
                 device already on it is unaffected."
            );
            eprintln!("  Run `gglib remote invite` again for a fresh code.");
        }
        Outcome::Interrupted => {
            eprintln!();
            eprintln!("  Left the pairing screen. The tunnel is still up; nothing was retired.");
        }
    }
    print_notice(offered.mcp_allowed, Arming::Invite);
    Ok(())
}

/// The pairing as plain text: for scripts, pipes, and terminals that cannot
/// draw. Everything printed here is a credential for two minutes.
fn print_plain(offered: &RemoteEnableDto) {
    let (Some(pairing), Some(code), Some(expires)) = (
        offered.pairing.as_deref(),
        offered.code.as_deref(),
        offered.expires_in_s,
    ) else {
        // The daemon answered an invite without one, which it should not.
        // Saying so beats printing a pairing screen with holes in it.
        eprintln!("  The daemon offered no pairing code. Nothing was handed out.");
        return;
    };
    println!("pairing: {pairing}");
    println!("ticket:  {}", offered.ticket);
    println!("code:    {code}");
    eprintln!();
    eprintln!("  On the other machine, within {expires}s:");
    eprintln!("    gglib remote join {pairing}");
}
