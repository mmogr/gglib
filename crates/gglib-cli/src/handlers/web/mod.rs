//! Web server command handler.
//!
//! Handles starting the Axum HTTP server with optional static file serving.
//! Discovers frontend build artifacts automatically from well-known paths,
//! or falls back to API-only mode when no frontend is present.
//!
//! Bind-address and CORS resolution lives in [`bind`]; this module orchestrates
//! it, prints the startup banner, and blocks on the server. When LAN sharing is
//! active it also advertises the server over mDNS ([`mdns`]) and races the
//! server against a shutdown signal ([`shutdown`]) so the record is withdrawn
//! on the way out.

mod bind;
mod mdns;
mod shutdown;

use std::path::PathBuf;

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::presentation::style;

/// Execute the `web` command.
///
/// Resolves the bind address and CORS policy from the flags and stored
/// settings, builds the Axum `ServerConfig`, resolves the static-files
/// directory (explicit flag → auto-discovery → API-only), prints startup
/// information, and then blocks until the server shuts down.
///
/// # Arguments
///
/// * `ctx`        — CLI context; supplies the stored binding preferences.
/// * `port`       — TCP port to listen on for HTTP requests.
/// * `host`       — Explicit bind address; `None` falls back to settings.
/// * `share_lan`  — Expose on all LAN interfaces and relax CORS to allow all.
/// * `base_port`  — Starting port range for llama-server subprocess allocation.
/// * `api_only`   — When `true`, skip static-file serving regardless of flags.
/// * `static_dir` — Explicit path to a built frontend; takes priority over
///   auto-discovery when `api_only` is `false`.
pub async fn execute(
    ctx: &CliContext,
    port: u16,
    host: Option<String>,
    share_lan: bool,
    base_port: u16,
    api_only: bool,
    static_dir: Option<PathBuf>,
) -> Result<()> {
    use gglib_axum::{ServerConfig, start_server};
    use gglib_core::paths::llama_server_path;

    // Warn if the VITE env var is set but unparseable so the user knows
    // we are ignoring it rather than silently falling back to the default.
    if let Ok(env_port) = std::env::var("VITE_GGLIB_WEB_PORT")
        && env_port.parse::<u16>().is_err()
    {
        eprintln!(
            "Warning: VITE_GGLIB_WEB_PORT='{}' is not a valid port number. Using default: {}",
            env_port, port
        );
    }

    let settings = ctx.app.settings().get().await?;
    let decision = bind::resolve_bind(host, share_lan, &settings)?;

    let mut config = ServerConfig {
        host: decision.host.clone(),
        port,
        base_port,
        llama_server_path: llama_server_path()?,
        max_concurrent: 4,
        max_concurrent_agent_loops: 4,
        static_dir: None,
        cors: decision.cors,
    };

    // Resolve static directory: api-only flag > explicit flag > auto-discover > none
    if !api_only {
        if let Some(dir) = static_dir {
            config.static_dir = Some(dir);
        } else {
            // Prefer built assets; accept the first directory that contains index.html.
            let candidates = ["./web_ui/dist", "./dist", "./web_ui/assets", "./web_ui"];
            for candidate in &candidates {
                let path = std::path::Path::new(candidate);
                if path.join("index.html").exists() {
                    config.static_dir = Some(path.to_path_buf());
                    break;
                }
            }
        }
    }

    if let Some(ref dir) = config.static_dir {
        style::print_info_banner("Web Server", "\u{1f680}");
        eprintln!("  \u{1f4c2} Serving UI from: {}", dir.display());
        if bind::is_wildcard(&config.host) {
            eprintln!(
                "  \u{1f310} Network: http://{}",
                bind::http_authority(&config.host, port)
            );
        } else {
            eprintln!(
                "  \u{1f310} Local:   http://{}",
                bind::http_authority(&config.host, port)
            );
        }
        eprintln!(
            "  \u{1f4ca} Status:  http://localhost:{}/v1/proxy/status",
            port
        );
        eprintln!();
        eprintln!("  Press Ctrl+C to stop");
        style::print_banner_close();
    } else {
        style::print_info_banner("Web Server (API only)", "\u{1f680}");
        eprintln!("  \u{1f310} API:     http://localhost:{}", port);
        eprintln!(
            "  \u{1f4ca} Status:  http://localhost:{}/v1/proxy/status",
            port
        );
        eprintln!();
        eprintln!("  \u{1f4a1} Tip: Use --static-dir to serve a frontend build");
        style::print_banner_close();
    }

    if !decision.share_lan {
        // Localhost-only: no advertising, and nothing to tear down, so the
        // server owns the process until it exits.
        start_server(config).await?;
        return Ok(());
    }

    print_share_lan_warning(&decision.host, port);

    // Registered just before the listener opens. `start_server` owns the bind,
    // so "after bind" is not observable from here; the resulting window where
    // the service is advertised but not yet accepting is sub-millisecond, and
    // closing it would mean threading a callback through gglib-axum, which the
    // Tauri adapter also consumes.
    let advertiser = mdns::MdnsAdvertiser::start(&decision.host, port);
    if advertiser.is_none() {
        // The technical cause is logged by the advertiser; say plainly here what
        // it means, since the user asked for discovery and is not getting it.
        eprintln!(
            "  \u{26a0}\u{fe0f}  mDNS advertising unavailable — 'gglib.local' will not resolve."
        );
        eprintln!("     The server is still reachable at its IP address.");
        eprintln!();
    }

    let outcome = tokio::select! {
        res = start_server(config) => res,
        () = shutdown::shutdown_signal() => {
            eprintln!();
            eprintln!("  Shutting down web server...");
            Ok(())
        }
    };

    // Withdraw the record before propagating any server error, so a crash does
    // not leave a stale `gglib.local` cached across the network.
    if let Some(advertiser) = advertiser {
        advertiser.shutdown().await;
    }

    outcome?;
    Ok(())
}

/// Print the LAN-exposure warning.
///
/// Sharing drops the server out of its localhost-only posture and allows every
/// origin through CORS, so it is called out explicitly rather than buried in
/// the startup log.
fn print_share_lan_warning(host: &str, port: u16) {
    eprintln!();
    eprintln!("  \u{26a0}\u{fe0f}  LAN SHARING ENABLED (--share-lan)");
    eprintln!("     Bound to {host}:{port} — reachable by every device on your network.");
    eprintln!("     CORS is relaxed to allow all origins.");
    eprintln!("     GGLib has no authentication: only use this on networks you trust.");
    eprintln!();
}
