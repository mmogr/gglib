#![doc = include_str!("README.md")]

mod lock;
mod shutdown;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use gglib_core::{CorsConfig, DAEMON_PORT, paths::data_root};

use crate::bootstrap::{ServerConfig, bootstrap};
use crate::state::AppState;

pub use lock::{DaemonLock, LockError, LockInfo};
pub use shutdown::shutdown_signal;

/// CORS origins the daemon always allows.
///
/// The desktop WebView origins plus the Vite dev server — the browser-facing
/// clients that reach the daemon cross-origin. Same-origin requests (the SPA
/// the daemon itself serves) need no CORS at all.
pub fn daemon_cors_origins() -> Vec<String> {
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
    /// Directory with a built frontend to serve as an SPA. `None` means
    /// auto-discover; `Some` is an explicit location.
    pub static_dir: Option<PathBuf>,
}

impl Default for DaemonOptions {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            cors: CorsConfig::AllowOrigins(daemon_cors_origins()),
            static_dir: None,
        }
    }
}

/// Locate a built frontend next to the working directory.
///
/// The same candidate list `gglib web` has always used; the first directory
/// containing an `index.html` wins. `None` means the daemon serves the API
/// only — every route still works, there is just no dashboard page.
#[must_use]
pub fn discover_static_dir() -> Option<PathBuf> {
    let candidates = ["./web_ui/dist", "./dist", "./web_ui/assets", "./web_ui"];
    candidates
        .iter()
        .map(std::path::Path::new)
        .find_map(|p| p.join("index.html").exists().then(|| p.to_path_buf()))
}

/// Run the gglib daemon until a shutdown signal or `/api/daemon/shutdown`.
///
/// The sequence:
///
/// 1. Take the machine-wide [`DaemonLock`] — refusing with the running
///    daemon's pid/address if there is one.
/// 2. Sweep orphaned llama-server pidfiles left by a crashed daemon.
/// 3. [`bootstrap`](crate::bootstrap::bootstrap) the one `AxumContext` —
///    and with it the one `ProcessManager` on this machine.
/// 4. Bind `{host}:{DAEMON_PORT}` and serve the management API (+ SPA when a
///    frontend build is found).
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

    // 4. Router — SPA when a frontend build exists, API-only otherwise.
    let static_dir = opts.static_dir.clone().or_else(discover_static_dir);
    let app = match &static_dir {
        Some(dir) => {
            info!("serving dashboard from {}", dir.display());
            crate::routes::create_spa_router(Arc::clone(&state), dir, &opts.cors)
        }
        None => crate::routes::create_router(Arc::clone(&state), &opts.cors),
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
    let graceful = {
        let token = shutdown_token.clone();
        async move {
            tokio::select! {
                () = shutdown::shutdown_signal() => info!("shutdown signal received"),
                () = token.cancelled() => info!("shutdown requested over the API"),
            }
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(graceful)
        .await?;

    shutdown::perform_shutdown(&state).await;

    Ok(())
}
