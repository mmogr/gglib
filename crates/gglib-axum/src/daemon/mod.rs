#![doc = include_str!("README.md")]

mod lock;
mod shutdown;
mod watchdog;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use gglib_core::{CorsConfig, DAEMON_PORT, paths::data_root};

use crate::bootstrap::{ServerConfig, bootstrap};
use crate::state::AppState;

// Stays `pub` because `lib.rs` re-exports it — a re-export chain must be public
// the whole way, and demoting this link is E0365. The lint tracks the chain
// correctly; it only fires if the root re-export is missing.
//
// It did fire here once, and rightly: the line used to read
// `pub use lock::{DaemonLock, LockError, LockInfo}`, and the other two are named
// nowhere in the workspace. The lint pointed at those; the `pub` it offers to
// demote covers the whole statement, so applying that fix took `DaemonLock` with
// them. They are gone now, and the line is honest.
pub use lock::DaemonLock;

/// CORS origins the daemon always allows.
///
/// The desktop WebView origins plus the Vite dev server — the browser-facing
/// clients that reach the daemon cross-origin. Same-origin requests (the SPA
/// the daemon itself serves) need no CORS at all.
pub(crate) fn daemon_cors_origins() -> Vec<String> {
    vec![
        "tauri://localhost".into(),
        "http://tauri.localhost".into(),
        "https://tauri.localhost".into(),
        "http://localhost:5173".into(), // Vite dev server
    ]
}

/// How the daemon binds and what it serves alongside the API.
#[derive(Debug, Clone)]
pub struct DaemonOptions {
    /// Bind host. The default is loopback; anything else is an explicit,
    /// eyes-open decision (`gglib daemon run --share-lan`).
    pub host: String,
    /// CORS policy for `/api`.
    pub cors: CorsConfig,
    /// Directory with a built frontend to serve as an SPA. `None` — the only
    /// value anything in this workspace sets — means serve the dashboard
    /// compiled into this binary. Nothing sets it implicitly, so the working
    /// directory can no longer decide what is served.
    ///
    /// `Some` overrides the embed with a directory. Note that no CLI flag or
    /// env var reaches this today: it is settable only by constructing
    /// `DaemonOptions` directly. It is kept because pointing a release build at
    /// a local `npm run build` is the obvious thing to want, and wiring a flag
    /// is a small change — but until one exists, calling it "the frontend
    /// developer's override" overstates what is here.
    pub static_dir: Option<PathBuf>,
    /// `Host` header values accepted in addition to loopback (and, on a
    /// non-loopback bind, IP literals). The mDNS name and `--allowed-host`
    /// entries arrive here.
    pub allowed_hosts: Vec<String>,
}

impl Default for DaemonOptions {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            cors: CorsConfig::AllowOrigins(daemon_cors_origins()),
            static_dir: None,
            allowed_hosts: Vec::new(),
        }
    }
}

/// Run the gglib daemon until a shutdown signal or `/api/daemon/shutdown`.
///
/// The sequence:
///
/// 1. Take the machine-wide [`DaemonLock`] — refusing with the running
///    daemon's pid/address if there is one.
/// 2. Sweep orphaned llama-server pidfiles left by a crashed daemon.
/// 3. [`bootstrap`] the one `AxumContext` —
///    and with it the one `ProcessManager` on this machine.
/// 4. Resolve the access policy — Host allowlist always, bearer token for
///    non-loopback binds — then bind `{host}:{DAEMON_PORT}` and serve the
///    management API (+ SPA when a frontend build is found).
/// 5. Honour `proxy_autostart` so the OpenAI endpoint comes up with the
///    daemon rather than with the desktop app.
/// 6. On SIGINT/SIGTERM/shutdown-route: drain the proxy, stop every child,
///    audit pidfiles — under a force-exit watchdog.
///
/// # Errors
///
/// Fails when the lock is held by another daemon, when the port is occupied
/// by a foreign process, or when bootstrap cannot build the service graph.
pub async fn run_daemon(opts: DaemonOptions) -> Result<()> {
    // 1. Singleton lock, before any side effect.
    let lock_dir = data_root().context("resolving data root for the daemon lock")?;
    let _lock = DaemonLock::acquire(&lock_dir, DAEMON_PORT).map_err(|e| anyhow!("{e}"))?;

    // 2. Orphan sweep. Only the daemon does this — it holds the lock, so any
    //    recorded llama-server pid is from a dead process, never a live
    //    sibling's. (This used to live in the desktop app's startup, where it
    //    killed servers a concurrently running CLI had just spawned.)
    if let Err(e) = gglib_runtime::pidfile::cleanup_orphaned_servers().await {
        warn!("orphan sweep at startup failed: {e}");
    }

    // 3. One context, one ProcessManager.
    let config = ServerConfig {
        host: opts.host.clone(),
        port: DAEMON_PORT,
        ..ServerConfig::with_defaults()?
    };
    let mut ctx = bootstrap(config).await?;

    let shutdown_token = CancellationToken::new();
    ctx.daemon_shutdown = Some(shutdown_token.clone());
    let state: AppState = Arc::new(ctx);

    // 4. Access policy, then the router. The Host guard is always on; the
    //    bearer token exists only for non-loopback binds, where the socket
    //    stops being the boundary.
    let api_key = resolve_daemon_api_key(&opts.host, &state).await;
    let access = Arc::new(crate::access::DaemonAccess::new(
        api_key,
        &opts.host,
        opts.allowed_hosts.clone(),
    ));

    // Router. The dashboard is compiled in (see `crate::ui`); a directory is
    // only ever an explicit override. This used to probe the *working
    // directory* for `./web_ui` and friends, which meant a release tarball run
    // from anywhere but its own extract directory silently served no
    // dashboard — and because the daemon is a singleton, whichever client
    // started it decided that for everyone.
    let app = if let Some(dir) = opts.static_dir.clone() {
        info!(
            "serving dashboard from {} (explicit override)",
            dir.display()
        );
        crate::routes::create_spa_router(Arc::clone(&state), &dir, &opts.cors, access)
    } else if crate::ui::has_embedded_ui() {
        info!("serving the dashboard compiled into this binary");
        crate::ui::create_embedded_spa_router(Arc::clone(&state), &opts.cors, access)
    } else {
        warn!(
            "this binary carries no dashboard — it was built without `npm run build`. \
             Serving the API only; every route works, there is just no dashboard page."
        );
        crate::routes::create_router(Arc::clone(&state), &opts.cors, access)
    };

    let addr = format!("{}:{}", opts.host, DAEMON_PORT);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| {
            format!(
                "could not bind {addr} — another program holds the port \
                 (the daemon lock was free, so it is not a gglib daemon)"
            )
        })?;
    info!("gglib daemon listening on http://{addr}");

    // From here the daemon can wedge in the worst way — listening but not
    // serving — so from here it watches itself. See the module docs for why
    // this must be a plain thread probing over TCP.
    match listener.local_addr() {
        Ok(bound) => watchdog::spawn(bound),
        Err(e) => warn!("no local addr for the liveness watchdog: {e}"),
    }

    // 5. The proxy belongs to the daemon now: honour autostart here, not in
    //    the desktop app.
    match state.core.settings().get().await {
        Ok(settings) if settings.proxy_autostart == Some(true) => {
            let proxy = Arc::clone(&state.proxy);
            tokio::spawn(async move {
                match proxy.ensure_running().await {
                    Ok(addr) => info!("proxy autostarted on {addr}"),
                    Err(e) => warn!("proxy autostart failed: {e}"),
                }
            });
        }
        Ok(_) => {}
        Err(e) => warn!("could not read settings for proxy autostart: {e}"),
    }

    // 6. Serve until signalled, then tear down in order.
    let graceful = shutdown::await_shutdown(shutdown::shutdown_signal(), shutdown_token.clone());

    axum::serve(listener, app)
        .with_graceful_shutdown(graceful)
        .await?;

    shutdown::perform_shutdown(&state).await;

    Ok(())
}

/// Settle the bearer token for a daemon about to bind `bind_host`.
///
/// Same policy as the proxy supervisor's key resolution: loopback binds get
/// no token (the socket is the boundary, and demanding one would break every
/// existing local client for no gain); anything else uses the stored
/// `proxy_api_key` — one machine key across both surfaces — minting and
/// persisting a fresh one when the setting is empty. The key is printed so a
/// `--share-lan` operator can copy it; the eprintln lands in `daemon.log`
/// for detached daemons, which cannot bind non-loopback today anyway.
async fn resolve_daemon_api_key(bind_host: &str, state: &AppState) -> Option<String> {
    if gglib_core::access::is_loopback_host(bind_host) {
        return None;
    }

    let settings = state.core.settings();
    let stored = match settings.get().await {
        Ok(s) => s,
        Err(e) => {
            // Fail closed with an unsaved key: an unauthenticated management
            // API on a network is strictly worse than a token that changes
            // next run.
            warn!("could not read settings while resolving the daemon API key: {e}");
            let key = gglib_core::access::generate_api_key();
            announce_api_key(&key, "generated, not saved");
            return Some(key);
        }
    };

    if let Some(key) = stored
        .proxy_api_key
        .clone()
        .filter(|key| !key.trim().is_empty())
    {
        announce_api_key(&key, "from settings");
        return Some(key);
    }

    let key = gglib_core::access::generate_api_key();
    let mut updated = stored;
    updated.proxy_api_key = Some(key.clone());
    match settings.save(&updated).await {
        Ok(()) => announce_api_key(&key, "generated"),
        Err(e) => {
            warn!("generated a daemon API key but could not save it: {e}");
            announce_api_key(&key, "generated, not saved");
        }
    }
    Some(key)
}

/// Print the management-API key at startup, once, where the operator who
/// chose a non-loopback bind will see it.
fn announce_api_key(key: &str, source: &str) {
    info!("management API requires a bearer token ({source})");
    eprintln!("  \u{1f511} Management API key ({source}): {key}");
    eprintln!("     Clients on the network must send: Authorization: Bearer {key}");
}
