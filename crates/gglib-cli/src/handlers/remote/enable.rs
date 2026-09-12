//! `gglib remote enable`.

use std::io::IsTerminal as _;

use anyhow::Result;

use super::pairing_tui::{self, Outcome};
use crate::bootstrap::CliContext;
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
    /// Offer a pairing code as well, so a first run is one command.
    pub invite: bool,
}

/// Execute `gglib remote enable`.
///
/// Ensures the daemon is running, asks it to bring the tunnel up, and shows
/// the pairing once — in the alternate screen when stdout is a terminal, as
/// plain text otherwise.
pub(crate) async fn enable(ctx: &CliContext, args: EnableArgs) -> Result<()> {
    let handle =
        daemon_client::ensure_daemon(daemon_client::auth::daemon_api_key(ctx).await).await?;

    if args.no_discovery {
        // Worth two lines now rather than one. The ticket used to die at the
        // next restart, so "stops working" meant "until you enable again".
        // It lasts now, and a ticket minted without discovery keeps only the
        // paths it had — so this is the flag that can leave a device holding
        // an address that will never resolve again.
        eprintln!(
            "  note: --no-discovery means this ticket carries only the paths it was minted with."
        );
        eprintln!(
            "        It stops resolving the moment this machine changes network, and stays that \
             way — paired devices need a new pairing."
        );
    }
    eprintln!("  Enabling remote access\u{2026} (finding a relay can take a few seconds)");
    let enabled = handle
        .remote_enable(&RemoteEnableBody {
            allow_mcp: args.allow_mcp,
            relay: args.relay,
            discovery: Some(!args.no_discovery),
            invite: args.invite,
        })
        .await?;

    // The daemon says whether it armed a session or answered from one that
    // was already running; this is not inferred. Inference from `mcp_allowed`
    // would catch only "asked for /mcp and told no" and leave the commoner
    // `enable --invite` with no flags reading as a fresh arm — which is the
    // case that goes on to recommend `--allow-mcp`, a flag that cannot take
    // on a session it did not arm.
    let arming = if enabled.already_up {
        // Every flag sent with this call was ignored, so say so for the one
        // that has a visible effect and that someone passes on purpose.
        if args.allow_mcp && !enabled.mcp_allowed {
            eprintln!(
                "  note: --allow-mcp did not take. Remote access was already on, and a \
                 session's /mcp grant"
            );
            eprintln!(
                "        belongs to the enable that armed it \u{2014} `gglib remote disable`, \
                 then `enable --allow-mcp`."
            );
        }
        Arming::Invite
    } else {
        Arming::Enable
    };

    // No code asked for, so there is nothing to show and nothing to wait on.
    // Rendering the absence as a pairing would put an expired-looking one on
    // screen at every enable.
    if enabled.code.is_none() {
        print_up(&enabled);
        print_notice(enabled.mcp_allowed, arming);
        return Ok(());
    }

    if args.no_qr || !std::io::stdout().is_terminal() {
        print_plain(&enabled);
        print_notice(enabled.mcp_allowed, arming);
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
                "  It holds a key of its own now; the tunnel stays up until \
                 `gglib remote disable`, and forgetting that device retires only its key."
            );
        }
        Outcome::Expired => {
            eprintln!();
            eprintln!(
                "  The pairing code expired and nobody paired. The tunnel is up, and every \
                 device that already holds a key of its own is unaffected."
            );
            eprintln!("  Run `gglib remote invite` again for a fresh code.");
        }
        Outcome::Ended { tunnel_up } => {
            eprintln!();
            for line in pairing_tui::withdrawn_notice(tunnel_up) {
                eprintln!("{line}");
            }
            if !tunnel_up {
                // The closing notice would say remote access is on and how to
                // stop broadcasting, and neither is true now. What this
                // `enable` did to the local proxy still is, and is said every
                // time.
                if arming == Arming::Enable {
                    eprintln!();
                    print_key_notice();
                }
                return Ok(());
            }
        }
        Outcome::Interrupted => {
            eprintln!();
            eprintln!(
                "  Left the pairing screen. The tunnel is still up; `gglib remote disable` stops it."
            );
        }
    }
    print_notice(enabled.mcp_allowed, arming);
    Ok(())
}

/// The pairing as plain text: for scripts, pipes, and terminals that cannot
/// draw. Everything printed here is a credential for two minutes.
fn print_plain(enabled: &RemoteEnableDto) {
    let (Some(pairing), Some(code), Some(expires)) = (
        enabled.pairing.as_deref(),
        enabled.code.as_deref(),
        enabled.expires_in_s,
    ) else {
        print_up(enabled);
        return;
    };
    println!("pairing: {pairing}");
    println!("ticket:  {}", enabled.ticket);
    println!("code:    {code}");
    eprintln!();
    eprintln!("  On the other machine, within {expires}s:");
    eprintln!("    gglib remote join {pairing}");
}

/// The tunnel is up and no device is being paired right now.
fn print_up(enabled: &RemoteEnableDto) {
    eprintln!("  Remote access is on, and stays on across restarts.");
    eprintln!("  Ticket:  {}", enabled.ticket);
    eprintln!("  No device is being paired. `gglib remote invite` offers a code.");
}

/// What enabling changed on *this* machine, said every time.
///
/// Takes the daemon's answer rather than the flag this process sent. An
/// `--invite` against a tunnel that is already up is answered by that
/// session, whose `/mcp` grant is whatever the `enable` that armed it set —
/// so echoing the flag back would tell an operator that `--allow-mcp` took
/// when it did not, or that `/mcp` is closed when it is open.
///
/// `Arming::Enable` says the switch was just thrown; `Arming::Invite` says it
/// was already on and only a code was offered. The difference is not
/// decoration: "the local proxy *now* requires the API key" and "pass
/// `--allow-mcp`" are both true of a switch being thrown and both false when
/// it was already on — where no flag changed, and recommending `--allow-mcp`
/// is recommending what the person just did.
///
/// So `Arming::Invite` is not only `invite`'s. `enable --invite` against a
/// tunnel that is already up is answered by that same session, and is the
/// same event under another name — which is why the daemon reports
/// `already_up` rather than leaving the caller to guess from what came back.
pub(super) fn print_notice(allow_mcp: bool, arming: Arming) {
    eprintln!();
    match arming {
        Arming::Enable => {
            eprintln!("  Remote access is on.");
            print_key_notice();
        }
        Arming::Invite => eprintln!("{INVITE_NOTICE}"),
    }
    match (allow_mcp, arming) {
        (true, _) => eprintln!("  /mcp is reachable through the tunnel."),
        (false, Arming::Enable) => {
            eprintln!(
                "  /mcp is not reachable through the tunnel; pass --allow-mcp to change that."
            );
        }
        // `invite` has no such flag: the grant belongs to the session `enable`
        // armed, and changing it means `disable` and `enable` again.
        (false, Arming::Invite) => {
            eprintln!(
                "  /mcp is not reachable through the tunnel; that is the session's setting, \
                 changed by `disable` and `enable --allow-mcp`."
            );
        }
    }
    eprintln!("  Stop broadcasting:  gglib remote disable");
}

/// What switching remote access on did to this machine's local proxy.
///
/// It stays true whatever becomes of the tunnel — the bearer requirement is
/// one `disable` does not take away — so it is said on its own after a
/// pairing screen that ended with the tunnel down, where "remote access is
/// on" would not be.
fn print_key_notice() {
    eprintln!(
        "  The local proxy on 127.0.0.1 now requires the API key too \u{2014} gglib's own \
         clients read it from settings; a hand-configured client needs it added once."
    );
    eprintln!(
        "  The daemon's own API on 127.0.0.1:9887 is unchanged \u{2014} this cannot lock \
         you out of `gglib` or the app."
    );
}

/// What `invite`, and `enable --invite` against a tunnel already up, say
/// about the session.
///
/// A constant so the wording is pinned by a test, as `DISABLE_NOTICE` is. It
/// said "this added a device", and it is printed after the pairing screen
/// whatever that screen saw, "expired and nobody paired" included. An invite
/// offers a code; whether a device took it is the line above's to say.
const INVITE_NOTICE: &str = "  Remote access was already on and still is; this offered a code for \
     one more device and changed nothing else about the session.";

/// Which command is printing the closing notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Arming {
    /// `enable`: the switch was just thrown.
    Enable,
    /// `invite`: it was already on, and a code for one more device was
    /// offered on it.
    Invite,
}

#[cfg(test)]
mod tests {
    use super::INVITE_NOTICE;

    /// An invite claims the code it offered, not a device that may never come:
    /// the notice follows "expired and nobody paired" as readily as "paired",
    /// so the old claim was false whenever nobody came.
    #[test]
    fn the_invite_notice_claims_a_code_and_not_a_device() {
        assert!(!INVITE_NOTICE.contains("added a device"), "{INVITE_NOTICE}");
        assert!(
            INVITE_NOTICE.contains("offered a code for one more device"),
            "{INVITE_NOTICE}"
        );
        assert!(
            INVITE_NOTICE.contains("changed nothing else about the session"),
            "{INVITE_NOTICE}"
        );
    }
}
