#![doc = include_str!("README.md")]

mod mdns;

use std::io::{IsTerminal as _, Write as _};

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonProbe};
use crate::presentation::style;
use crate::target::Target;
use gglib_axum::{DaemonLock, DaemonOptions, run_daemon};
use gglib_core::{CorsConfig, DAEMON_PORT};

/// Execute `gglib daemon run`: host the daemon in the foreground.
pub(crate) async fn run(share_lan: bool, allowed_hosts: Vec<String>) -> Result<()> {
    let opts = if share_lan {
        print_share_lan_warning();
        // The mDNS name is advertised below, so it is plainly a name this
        // daemon answers to — nobody should have to repeat it as a flag.
        let mut allowed_hosts = allowed_hosts;
        allowed_hosts.push(mdns::LAN_HOSTNAME.into());
        DaemonOptions {
            host: "0.0.0.0".into(),
            cors: CorsConfig::AllowAll,
            allowed_hosts,
            ..DaemonOptions::default()
        }
    } else {
        DaemonOptions {
            allowed_hosts,
            ..DaemonOptions::default()
        }
    };

    // Registered just before the daemon binds; every mDNS failure is
    // non-fatal (the server is reachable by IP either way).
    let advertiser = share_lan
        .then(|| mdns::MdnsAdvertiser::start(&opts.host, DAEMON_PORT))
        .flatten();

    let outcome = run_daemon(opts).await;

    // Withdraw the record before propagating any error, so a crash does not
    // leave a stale `gglib.local` cached across the network.
    if let Some(advertiser) = advertiser {
        advertiser.shutdown().await;
    }

    outcome
}

/// Execute `gglib daemon status`.
pub(crate) async fn status(ctx: &CliContext) -> Result<()> {
    let client = reqwest::Client::new();

    style::print_info_banner("Daemon", "\u{2139}\u{fe0f}");
    match daemon_client::probe(&client).await {
        DaemonProbe::Running => {
            eprintln!("  Status:  running at {}", daemon_client::base_url());
            if let Ok(dir) = gglib_core::paths::data_root()
                && let Some(holder) = DaemonLock::read_holder(&dir)
            {
                eprintln!("  PID:     {}", holder.pid);
            }
            let handle = daemon_client::DaemonHandle {
                client,
                api_key: daemon_client::auth::daemon_api_key(ctx).await,
            };
            match handle.proxy_status().await {
                Ok(proxy) if proxy.running => {
                    eprintln!(
                        "  Proxy:   running on port {}",
                        proxy.port.map_or_else(|| "?".into(), |p| p.to_string())
                    );
                    if let Some(pinned) = proxy.pinned_model {
                        eprintln!("  Pinned:  {pinned}");
                    }
                }
                Ok(_) => eprintln!("  Proxy:   stopped"),
                Err(e) => eprintln!("  Proxy:   status unavailable ({e})"),
            }
        }
        DaemonProbe::NotRunning => {
            eprintln!("  Status:  not running");
            eprintln!("  Start it with any runtime command, or `gglib daemon run`.");
        }
        DaemonProbe::ForeignServer => {
            eprintln!(
                "  Status:  port {DAEMON_PORT} is held by another program (not a gglib daemon)"
            );
        }
    }
    style::print_banner_close();
    Ok(())
}

/// Execute `gglib daemon stop`: this machine's daemon, or with `--remote`
/// the paired machine's, through the tunnel.
pub(crate) async fn stop(ctx: &CliContext, target: Target, yes: bool) -> Result<()> {
    target
        .run(
            async || stop_here(ctx).await,
            async || stop_far(ctx, yes).await,
        )
        .await
}

/// Request shutdown from the daemon on this machine and wait for it to land.
async fn stop_here(ctx: &CliContext) -> Result<()> {
    let client = reqwest::Client::new();

    match daemon_client::probe(&client).await {
        DaemonProbe::NotRunning => {
            eprintln!("  Daemon is not running.");
            return Ok(());
        }
        DaemonProbe::ForeignServer => anyhow::bail!(
            "port {DAEMON_PORT} is held by another program (not a gglib daemon) — nothing to stop"
        ),
        DaemonProbe::Running => {}
    }

    let handle = daemon_client::DaemonHandle {
        client,
        api_key: daemon_client::auth::daemon_api_key(ctx).await,
    };
    if !handle.shutdown_daemon().await? {
        anyhow::bail!(
            "the server on port {DAEMON_PORT} refused the shutdown request \
             (not running as a daemon)"
        );
    }

    // The daemon tears down llama-server children before exiting; give it
    // the same window its own shutdown watchdog enforces.
    eprint!("  Stopping daemon");
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if matches!(
            daemon_client::probe(&handle.client).await,
            DaemonProbe::NotRunning
        ) {
            eprintln!(" \u{2014} stopped.");
            return Ok(());
        }
        eprint!(".");
    }
    eprintln!();
    anyhow::bail!("the daemon accepted the shutdown request but is still answering after 20s")
}

/// The LAN-exposure warning, printed before a `--share-lan` daemon binds.
fn print_share_lan_warning() {
    eprintln!();
    eprintln!("  \u{26a0}\u{fe0f}  LAN SHARING ENABLED (--share-lan)");
    eprintln!("     The daemon is reachable by every device on your network.");
    eprintln!("     Its management API requires the API key printed below \u{2014} anyone");
    eprintln!("     holding it can download models and start or stop inference on");
    eprintln!("     this machine. Only use this on networks you trust.");
    eprintln!();
}

/// Stop the paired machine's daemon through the tunnel, then disconnect.
///
/// Asks first, because the far side cannot be restarted from here. `--yes`
/// skips the question; so does a stdin that is not a terminal, on the theory
/// that a script passing `--remote --yes` has read the help. The stop itself
/// is this machine's daemon's to carry out: it owns the tunnel, it already
/// does this for the desktop app, and the request it sends asks the far
/// proxy to type the same word.
async fn stop_far(ctx: &CliContext, yes: bool) -> Result<()> {
    let client = reqwest::Client::new();
    match daemon_client::probe(&client).await {
        DaemonProbe::Running => {}
        _ => anyhow::bail!("the daemon is not running, so nothing is connected to a remote"),
    }
    let handle = daemon_client::DaemonHandle {
        client,
        api_key: daemon_client::auth::daemon_api_key(ctx).await,
    };
    let status = handle.remote_status().await?;
    let Some(connection) = status.connected.as_ref() else {
        anyhow::bail!("not connected to a remote \u{2014} `gglib remote join` first");
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
