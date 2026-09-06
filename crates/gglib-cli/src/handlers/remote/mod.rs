#![doc = include_str!("README.md")]

mod connect;
mod enable;
mod pairing_tui;

use connect::{ConnectArgs, connect, disconnect, kill};
use enable::{EnableArgs, enable};

use anyhow::Result;

use crate::commands::RemoteCommand;
use crate::daemon_client::{self, DaemonProbe, RemoteStatusDto};
use crate::presentation::style;

/// Route a `gglib remote` subcommand to its handler.
pub(crate) async fn dispatch(command: RemoteCommand) -> Result<()> {
    match command {
        RemoteCommand::Enable {
            allow_mcp,
            relay,
            no_discovery,
            no_qr,
        } => {
            enable(EnableArgs {
                allow_mcp,
                relay,
                no_discovery,
                no_qr,
            })
            .await
        }
        RemoteCommand::Disable => disable().await,
        RemoteCommand::Status => status().await,
        RemoteCommand::Connect {
            pairing,
            port,
            relay,
            no_discovery,
        } => {
            connect(ConnectArgs {
                pairing,
                port,
                relay,
                no_discovery,
            })
            .await
        }
        RemoteCommand::Disconnect => disconnect().await,
        RemoteCommand::Kill { yes } => kill(yes).await,
    }
}

/// Execute `gglib remote disable`.
pub(crate) async fn disable() -> Result<()> {
    let client = reqwest::Client::new();
    match daemon_client::probe(&client).await {
        DaemonProbe::Running => {}
        _ => {
            eprintln!("  Daemon is not running \u{2014} nothing is being broadcast.");
            return Ok(());
        }
    }
    let handle = daemon_client::DaemonHandle { client };
    let status = handle.remote_disable().await?;
    if status.enabled {
        anyhow::bail!("the daemon reported remote access still enabled after disable");
    }
    eprintln!("  Remote access is off. The ticket is dead; a later `enable` mints a new one.");
    eprintln!(
        "  The API key stays in settings \u{2014} authentication turns on and never off by itself."
    );
    Ok(())
}

/// Execute `gglib remote status`.
pub(crate) async fn status() -> Result<()> {
    let client = reqwest::Client::new();
    style::print_info_banner("Remote", "\u{1f517}");
    match daemon_client::probe(&client).await {
        DaemonProbe::Running => {}
        DaemonProbe::NotRunning => {
            eprintln!("  Daemon:  not running \u{2014} nothing is being broadcast");
            style::print_banner_close();
            return Ok(());
        }
        DaemonProbe::ForeignServer => {
            eprintln!("  Daemon:  another program holds the daemon port");
            style::print_banner_close();
            return Ok(());
        }
    }
    let handle = daemon_client::DaemonHandle { client };
    let status = handle.remote_status().await?;
    print_status(&status);
    style::print_banner_close();
    Ok(())
}

/// The status, one line per fact.
fn print_status(status: &RemoteStatusDto) {
    if !status.enabled {
        eprintln!("  Serving:   off \u{2014} `gglib remote enable` to broadcast this machine");
    } else {
        eprintln!(
            "  Serving:   on   (ticket {})",
            status.ticket_fingerprint.as_deref().unwrap_or("?")
        );
        eprintln!(
            "  Pairing:   {}",
            match (status.pairing_active, status.paired) {
                (true, _) => "code live, waiting for a device",
                (false, true) => "paired",
                (false, false) => "code expired or spent, nobody paired",
            }
        );
        eprintln!("  Path:      {}", status.path.as_deref().unwrap_or("idle"));
        if status.peers.is_empty() {
            eprintln!("  Peers:     none connected");
        } else {
            for peer in &status.peers {
                eprintln!("  Peer:      {}  ({})", peer.fingerprint, peer.path);
            }
        }
        eprintln!(
            "  /mcp:      {}",
            if status.mcp_allowed {
                "reachable through the tunnel"
            } else {
                "not reachable through the tunnel"
            }
        );
    }
    connect::print_connection(status);
    eprintln!(
        "  Requests:  {} through the tunnel",
        status.tunnelled_requests
    );
    if let Some(ms) = status.last_tunnelled_ms {
        eprintln!(
            "  Last one:  {}{}",
            ago(ms),
            status
                .last_peer
                .as_deref()
                .map(|p| format!(", from {p}"))
                .unwrap_or_default()
        );
    }
}

/// A unix-millisecond timestamp as "N seconds/minutes/hours ago".
fn ago(unix_ms: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(unix_ms);
    let secs = (now - unix_ms).max(0) / 1000;
    match secs {
        s if s < 60 => format!("{s}s ago"),
        s if s < 3600 => format!("{}m ago", s / 60),
        s => format!("{}h ago", s / 3600),
    }
}
