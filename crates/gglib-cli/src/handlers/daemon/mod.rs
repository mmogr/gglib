#![doc = include_str!("README.md")]

mod mdns;

use anyhow::Result;

use crate::daemon_client::{self, DaemonProbe};
use crate::presentation::style;
use gglib_axum::daemon::{DaemonLock, DaemonOptions, run_daemon};
use gglib_core::{CorsConfig, DAEMON_PORT};

/// Execute `gglib daemon run`: host the daemon in the foreground.
pub async fn run(share_lan: bool, allowed_hosts: Vec<String>) -> Result<()> {
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
pub async fn status() -> Result<()> {
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
            let handle = daemon_client::DaemonHandle { client };
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

/// Execute `gglib daemon stop`: request shutdown and wait for it to land.
pub async fn stop() -> Result<()> {
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

    let handle = daemon_client::DaemonHandle { client };
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
