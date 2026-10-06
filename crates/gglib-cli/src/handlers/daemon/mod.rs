#![doc = include_str!("README.md")]

mod mdns;

use std::io::{IsTerminal as _, Write as _};

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, Absent, DaemonProbe};
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
    style::print_info_banner("Daemon", "\u{2139}\u{fe0f}");
    match daemon_client::running(ctx).await {
        Ok(handle) => {
            eprintln!("  Status:  running at {}", daemon_client::base_url());
            if let Ok(dir) = gglib_core::paths::data_root()
                && let Some(holder) = DaemonLock::read_holder(&dir)
            {
                eprintln!("  PID:     {}", holder.pid);
            }
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
        Err(Absent::NotRunning) => {
            eprintln!("  Status:  not running");
            eprintln!("  Start it with any runtime command, or `gglib daemon run`.");
        }
        Err(Absent::ForeignServer) => {
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
    let handle = match daemon_client::running(ctx).await {
        Ok(handle) => handle,
        Err(Absent::NotRunning) => {
            eprintln!("  Daemon is not running.");
            return Ok(());
        }
        Err(Absent::ForeignServer) => anyhow::bail!(
            "port {DAEMON_PORT} is held by another program (not a gglib daemon) — nothing to stop"
        ),
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
    let Ok(handle) = daemon_client::running(ctx).await else {
        anyhow::bail!("the daemon is not running, so nothing is connected to a remote")
    };
    let status = handle.remote_status().await?;
    if status.connected.is_none() {
        anyhow::bail!("not connected to a remote \u{2014} `gglib remote join` first");
    }

    if !yes && std::io::stdin().is_terminal() && !confirm(status.paired_shown())? {
        eprintln!("  Left it running.");
        return Ok(());
    }
    handle.remote_kill().await?;
    eprintln!("{}", stopping_line(status.paired_shown()));
    eprintln!("  Nothing brings it back except someone at that machine.");
    Ok(())
}

/// What a stop of the paired machine's daemon says once it is under way,
/// naming that machine as every other surface does: by its name, never its
/// fingerprint.
fn stopping_line(machine: &str) -> String {
    format!("  \u{1f6d1} The gglib daemon on {machine} is stopping, and this side is disconnected.")
}

/// What the question says the stop will take down, and on which machine.
fn question(machine: &str) -> String {
    format!("  This stops the gglib daemon on {machine}: its proxy, its models, its downloads.")
}

/// The question, and the one answer that means yes.
fn confirm(machine: &str) -> Result<bool> {
    eprintln!("{}", question(machine));
    eprintln!("  It cannot be started again from here.");
    eprint!("  Type `shutdown` to go ahead: ");
    std::io::stderr().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim() == "shutdown")
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod mod_tests;
