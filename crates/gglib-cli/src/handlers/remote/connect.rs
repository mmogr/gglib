//! `gglib remote connect`, `disconnect` and `kill`: this machine as the laptop.

use std::io::{IsTerminal as _, Write as _};

use anyhow::Result;

use crate::daemon_client::{self, DaemonProbe, RemoteConnectBody, RemoteStatusDto};

/// What `gglib remote connect` was asked for.
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
}

/// Execute `gglib remote connect`.
pub(crate) async fn connect(args: ConnectArgs) -> Result<()> {
    let handle = daemon_client::ensure_daemon().await?;
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
    eprintln!("  The other machine is now at:  {}", connected.base_url);
    eprintln!("  Any OpenAI-compatible client pointed there needs its API key; gglib's own do:");
    eprintln!("    gglib q --remote \"\u{2026}\"        gglib chat --remote");
    eprintln!();
    eprintln!("  Close it:  gglib remote disconnect");
    Ok(())
}

/// Execute `gglib remote disconnect`.
pub(crate) async fn disconnect() -> Result<()> {
    let client = reqwest::Client::new();
    match daemon_client::probe(&client).await {
        DaemonProbe::Running => {}
        _ => {
            eprintln!("  Daemon is not running \u{2014} nothing is connected.");
            return Ok(());
        }
    }
    let handle = daemon_client::DaemonHandle { client };
    let status = handle.remote_disconnect().await?;
    if status.connected.is_some() {
        anyhow::bail!("the daemon reported the connection still up after disconnect");
    }
    eprintln!("  Disconnected. The pairing is remembered; `gglib remote connect` dials it again.");
    Ok(())
}

/// Execute `gglib remote kill`.
///
/// Asks first, because the far side cannot be restarted from here. `--yes`
/// skips the question; so does a stdin that is not a terminal, on the theory
/// that a script passing `kill` has read the help.
pub(crate) async fn kill(yes: bool) -> Result<()> {
    let client = reqwest::Client::new();
    match daemon_client::probe(&client).await {
        DaemonProbe::Running => {}
        _ => {
            anyhow::bail!("the daemon is not running, so nothing is connected to a remote");
        }
    }
    let handle = daemon_client::DaemonHandle { client };
    let status = handle.remote_status().await?;
    let Some(connection) = status.connected.as_ref() else {
        anyhow::bail!("not connected to a remote \u{2014} `gglib remote connect` first");
    };

    if !yes && std::io::stdin().is_terminal() && !confirm(&connection.ticket_fingerprint)? {
        eprintln!("  Left it running.");
        return Ok(());
    }
    handle.remote_kill().await?;
    eprintln!(
        "  \u{1f6d1} The remote daemon ({}) is stopping, and this side is disconnected.",
        connection.ticket_fingerprint
    );
    eprintln!("  Nothing brings it back except someone at that machine.");
    Ok(())
}

/// The question, and the one answer that means yes.
fn confirm(fingerprint: &str) -> Result<bool> {
    eprintln!(
        "  This stops the gglib daemon on {fingerprint}: its proxy, its models, its downloads."
    );
    eprintln!("  It cannot be started again from here.");
    eprint!("  Type `shutdown` to go ahead: ");
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim() == "shutdown")
}

/// The connect side's lines of `gglib remote status`.
pub(super) fn print_connection(status: &RemoteStatusDto) {
    match &status.connected {
        Some(c) => {
            eprintln!(
                "  Connected: {}  at {}  ({})",
                c.ticket_fingerprint, c.base_url, c.path
            );
        }
        None => match (&status.stored_ticket_fingerprint, status.has_remote_key) {
            (Some(fp), true) => {
                eprintln!("  Connected: no \u{2014} `gglib remote connect` dials {fp} again");
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
