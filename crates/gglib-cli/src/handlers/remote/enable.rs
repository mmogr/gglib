//! `gglib remote enable`.

use std::io::IsTerminal as _;

use anyhow::{Result, bail};
use gglib_app_services::{RemoteEnableBody, RemoteEnableResponse};

use super::pairing_tui::{self, Outcome};
use crate::bootstrap::CliContext;
use crate::daemon_client;

/// What `gglib remote enable` was asked for.
#[derive(Debug, Clone, Default)]
pub(crate) struct EnableArgs {
    /// A self-hosted relay URL.
    pub relay: Option<String>,
    /// Do not publish to or resolve through n0's discovery service.
    pub no_discovery: bool,
    /// Print the pairing string as text; no QR, no alternate screen.
    pub no_qr: bool,
    /// Offer a pairing code as well, so a first run is one command.
    pub invite: bool,
}

/// What `enable --allow-mcp` is answered with. The flag named the MCP servers
/// a session opened to paired devices; each server now says so itself.
pub(super) const ALLOW_MCP_GONE: &str = "--allow-mcp is gone: run `gglib mcp allow-remote <names>`";

/// Refuse `--allow-mcp`, before the daemon is asked anything: a script that
/// still passes it learns where the grant went, and no tunnel comes up
/// opening less than it asked for.
pub(super) fn refuse_allow_mcp(allow_mcp: &[String]) -> Result<()> {
    if allow_mcp.is_empty() {
        return Ok(());
    }
    bail!(ALLOW_MCP_GONE)
}

/// Execute `gglib remote enable`.
///
/// Ensures the daemon is running, asks it to bring the tunnel up, and shows
/// the pairing once — in the alternate screen when stdout is a terminal, as
/// plain text otherwise.
pub(crate) async fn enable(ctx: &CliContext, args: EnableArgs) -> Result<()> {
    let handle = daemon_client::ensure_daemon(ctx).await?;

    eprintln!("  Enabling remote access\u{2026} (finding a relay can take a few seconds)");
    let enabled = handle.remote_enable(&body_of(&args)).await?;

    // After the answer, and only when this call armed the session: one that
    // was already up, or that the daemon's own resume brought back while this
    // waited, keeps the paths it was minted with whatever was passed here.
    if args.no_discovery && !enabled.already_up {
        // Worth two lines. The ticket lasts across restarts, and a ticket
        // minted without discovery keeps only the paths it had — so this is
        // the flag that can leave a device holding an address that will never
        // resolve again.
        eprintln!(
            "  note: --no-discovery means this ticket carries only the paths it was minted with."
        );
        eprintln!(
            "        It stops resolving the moment this machine changes network, and stays that \
             way — paired devices need a new pairing."
        );
    }

    // The daemon says whether it armed a session or answered from one that
    // was already running; this is not inferred.
    let arming = if enabled.already_up {
        // A code means `--invite` found a session to offer one on. None means
        // a plain `enable` that waited for the daemon's own resume, the only
        // way the daemon answers `already_up` without one.
        if enabled.code.is_some() {
            Arming::Invite
        } else {
            Arming::Resumed
        }
    } else {
        Arming::Enable
    };

    // No code asked for, so there is nothing to show and nothing to wait on.
    // Rendering the absence as a pairing would put an expired-looking one on
    // screen at every enable.
    if enabled.code.is_none() {
        print_up(&enabled);
        print_notice(&enabled.mcp_servers, arming);
        return Ok(());
    }

    if args.no_qr || !std::io::stdout().is_terminal() {
        print_plain(&enabled);
        print_notice(&enabled.mcp_servers, arming);
        return Ok(());
    }

    match pairing_tui::run(&handle, &enabled).await? {
        Outcome::Paired { device } => {
            eprintln!();
            eprintln!("{}", pairing_tui::paired_line(device.as_deref()));
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
    print_notice(&enabled.mcp_servers, arming);
    Ok(())
}

/// The pairing as plain text: for scripts, pipes, and terminals that cannot
/// draw. Everything printed here is a credential for two minutes.
fn print_plain(enabled: &RemoteEnableResponse) {
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
    if let Some(device) = enabled.device.as_deref() {
        eprintln!("{}", pairing_tui::code_is_for(device));
    }
}

/// The tunnel is up and no device is being paired right now.
fn print_up(enabled: &RemoteEnableResponse) {
    eprintln!("  Remote access is on, and stays on across restarts.");
    eprintln!("  Ticket:  {}", enabled.ticket);
    eprintln!("  No device is being paired. `gglib remote invite` offers a code.");
}

/// What enabling changed on *this* machine, said every time.
///
/// Takes the daemon's answer for the MCP servers open to paired devices,
/// which each server says itself; nothing this process sent names them.
///
/// `Arming::Enable` says the switch was just thrown; `Arming::Invite` says it
/// was already on and only a code was offered. The difference is not
/// decoration: "the local proxy *now* requires the API key" is true of a
/// switch being thrown and false when it was already on.
///
/// So `Arming::Invite` is not only `invite`'s. `enable --invite` against a
/// tunnel that is already up is answered by that same session, and is the
/// same event under another name — which is why the daemon reports
/// `already_up` rather than leaving the caller to guess from what came back.
///
/// `Arming::Resumed` is a plain `enable` answered from the session the
/// daemon's own resume brought back while it waited: the switch was already
/// on, nothing was offered, and no flag took.
pub(super) fn print_notice(servers: &[String], arming: Arming) {
    eprintln!();
    match arming {
        Arming::Enable => {
            eprintln!("  Remote access is on.");
            print_key_notice();
        }
        Arming::Invite => eprintln!("{INVITE_NOTICE}"),
        Arming::Resumed => eprintln!("{RESUMED_NOTICE}"),
    }
    eprintln!("{}", mcp_notice(servers));
    eprintln!("  Stop broadcasting:  gglib remote disable");
}

/// What `enable` asks the daemon for, from what it was asked.
fn body_of(args: &EnableArgs) -> RemoteEnableBody {
    RemoteEnableBody {
        relay: args.relay.clone(),
        discovery: Some(!args.no_discovery),
        invite: args.invite,
    }
}

/// What the closing notice says of the MCP servers open to paired devices,
/// whichever command printed it: each server says so itself, and a change
/// to one applies from a device's next run.
fn mcp_notice(servers: &[String]) -> String {
    if servers.is_empty() {
        "  No MCP server is open to paired devices; `gglib mcp allow-remote <server>` opens one."
            .to_owned()
    } else {
        format!(
            "  Paired devices' chats may use: {}. /mcp is never reachable through the tunnel.",
            servers.join(", ")
        )
    }
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

/// What a plain `enable` says when it was answered from the session the
/// daemon's own resume brought back. Nothing was switched on by it and no flag
/// took, so the key notice is not its to give. A constant, pinned by a test,
/// as `INVITE_NOTICE` is.
const RESUMED_NOTICE: &str = "  Remote access was already coming back up when you asked, and is on now; \
     this changed nothing about the session. `gglib remote disable`, then `enable`, changes its flags.";

/// Which command is printing the closing notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Arming {
    /// `enable`: the switch was just thrown.
    Enable,
    /// `invite`: it was already on, and a code for one more device was
    /// offered on it.
    Invite,
    /// A plain `enable` the daemon answered from the session its own resume
    /// brought back, after waiting for it.
    Resumed,
}

#[cfg(test)]
#[path = "enable_tests.rs"]
mod enable_tests;
