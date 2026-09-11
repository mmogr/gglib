#![doc = include_str!("README.md")]

mod connect;
mod enable;
mod pairing_tui;

use connect::{ConnectArgs, connect, disconnect};
use enable::{EnableArgs, enable};

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::commands::RemoteCommand;
use crate::daemon_client::{self, DaemonProbe, RemoteStatusDto};
use crate::presentation::style;

/// Route a `gglib remote` subcommand to its handler.
pub(crate) async fn dispatch(ctx: &CliContext, command: RemoteCommand) -> Result<()> {
    match command {
        RemoteCommand::Enable {
            allow_mcp,
            relay,
            no_discovery,
            no_qr,
        } => {
            enable(
                ctx,
                EnableArgs {
                    allow_mcp,
                    relay,
                    no_discovery,
                    no_qr,
                },
            )
            .await
        }
        RemoteCommand::Disable => disable(ctx).await,
        RemoteCommand::Status => status(ctx).await,
        RemoteCommand::Connect {
            pairing,
            port,
            relay,
            no_discovery,
        } => {
            connect(
                ctx,
                ConnectArgs {
                    pairing,
                    port,
                    relay,
                    no_discovery,
                },
            )
            .await
        }
        RemoteCommand::Disconnect => disconnect(ctx).await,
    }
}

/// Execute `gglib remote disable`.
pub(crate) async fn disable(ctx: &CliContext) -> Result<()> {
    let client = reqwest::Client::new();
    match daemon_client::probe(&client).await {
        DaemonProbe::Running => {}
        _ => {
            eprintln!("  Daemon is not running \u{2014} nothing is being broadcast.");
            return Ok(());
        }
    }
    let handle = daemon_client::DaemonHandle {
        client,
        api_key: daemon_client::auth::daemon_api_key(ctx).await,
    };
    let status = handle.remote_disable().await?;
    if status.enabled {
        anyhow::bail!("the daemon reported remote access still enabled after disable");
    }
    for line in DISABLE_NOTICE {
        eprintln!("{line}");
    }
    Ok(())
}

/// What `gglib remote disable` prints once the tunnel is down.
///
/// A constant so the wording is pinned by a test rather than by nobody. The
/// sentence this replaced — "authentication turns on and never off by itself"
/// — is true of a listener that bound with a key already in settings, and
/// false of the ordinary case this command ends: `enable` mints the key into a
/// proxy that is *already* bound on loopback, and a loopback bind resolves no
/// key, so that listener has no bind-time floor and clearing `proxy_api_key`
/// reopens it, `/mcp` included. `clearing_reopens_a_listener_that_bound_on_loopback`
/// in `gglib-core`'s `access::bearer_tests` asserts exactly that.
///
/// Which of the two a running listener is, this side cannot see: `disable`
/// talks to the daemon over HTTP and never learns the proxy's bind. So the
/// notice names the dependency and points at the command that shows the state,
/// rather than asserting a rule that holds in one case only. `docs/remote.md`
/// carries both cases in full, and ADR 0012 the reasoning.
const DISABLE_NOTICE: [&str; 4] = [
    "  Remote access is off. Nothing answers the ticket while it is off, and",
    "  `enable` brings the same one back \u{2014} revoking is deleting the endpoint key.",
    "  The API key stays in settings \u{2014} whether the proxy still demands it depends",
    "  on the bind its listener came up with. `gglib config settings show` prints it.",
];

/// Execute `gglib remote status`.
pub(crate) async fn status(ctx: &CliContext) -> Result<()> {
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
    let handle = daemon_client::DaemonHandle {
        client,
        api_key: daemon_client::auth::daemon_api_key(ctx).await,
    };
    let status = handle.remote_status().await?;
    print_status(&status);
    style::print_banner_close();
    Ok(())
}

/// The status, one line per fact.
fn print_status(status: &RemoteStatusDto) {
    if !status.enabled {
        // The switch being on with nothing bound is worth its own sentence:
        // it is a machine that failed to arm at boot, or is still arming,
        // and "off" alone would send someone to `enable` for a thing that
        // is already enabled.
        if status.remote_enabled {
            eprintln!(
                "  Serving:   switched on, but nothing is bound \u{2014} still arming, or it \
                 could not reach a relay"
            );
        } else {
            eprintln!("  Serving:   off \u{2014} `gglib remote enable` to broadcast this machine");
        }
    } else {
        eprintln!(
            "  Serving:   on   (ticket {})",
            status.ticket_fingerprint.as_deref().unwrap_or("?")
        );
        // The ticket lasts now, so where the key lives is operational
        // knowledge rather than trivia: deleting that file is how a ticket
        // is revoked, and it is the only way.
        if let Some(path) = &status.identity_path {
            eprintln!("  Identity:  lasting \u{2014} same ticket after a restart");
            eprintln!("             {path}");
        }
        if !status.remote_enabled {
            eprintln!(
                "  After a restart: off \u{2014} this tunnel was not switched on by `enable`"
            );
        }
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
    print_traffic(status);
}

/// The tunnelled-request count, printed only on the machine that counts it.
///
/// `tunnelled_requests` ticks in the proxy that *receives* tunnel-marked
/// requests, which is the serving machine's. A machine that only dials out
/// never sees one, so printing the count there reported a permanent `0` on a
/// perfectly healthy connection — read twice as "it isn't working" during the
/// first two-machine session. Keep this conditional: the connecting side is
/// told where the number lives instead of being handed a meaningless zero.
fn print_traffic(status: &RemoteStatusDto) {
    if !status.enabled {
        if status.connected.is_some() {
            eprintln!(
                "  Requests:  counted on the machine you dialled \u{2014} run `gglib remote status` there"
            );
        }
        return;
    }
    eprintln!(
        "  Requests:  {} served through the tunnel",
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

#[cfg(test)]
#[path = "mod_tests.rs"]
mod mod_tests;
