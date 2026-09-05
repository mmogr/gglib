//! Axum server bootstrap - the composition root for the Axum web adapter.
//!
//! Shared infrastructure (DB, runner, GGUF parser, model registrar/files,
//! HF client, download manager, model verification, AppCore-with-verification)
//! is wired by [`gglib_bootstrap::CoreBootstrap`]. This module adds the
//! Axum-specific layer on top: the SSE broadcaster (which doubles as the
//! `AppEventEmitter` for the shared bootstrap), the MCP service, the seven
//! domain ops, and the proxy crash watcher.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use gglib_app_services::{
    AppServices, BenchmarkOps, DownloadOps, McpOps, ModelOps, ProxyOps, RemoteOps, ServerOps,
    ServiceGraphParams, SettingsOps, SetupOps, build_service_graph,
};
use gglib_bootstrap::{BootstrapConfig, BuiltCore, CoreBootstrap};
use gglib_core::ports::{AppEventEmitter, HfClientPort, ModelCatalogPort, ModelRuntimePort};
use gglib_core::services::AppCore;
use gglib_db::SqliteBenchmarkRepository;
use gglib_db::cleanup_zombie_benchmark_runs;
use gglib_gguf::ToolSupportDetector;
use gglib_mcp::McpService;
use reqwest::Client;

use crate::sse::SseBroadcaster;

// Path utilities from core
use gglib_core::CorsConfig;
use gglib_core::paths::{
    data_root, database_path, llama_server_path, resolve_models_dir, resource_root,
};

/// Server configuration for the Axum adapter.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Host to bind the HTTP server.
    pub host: String,
    /// Port for the HTTP server.
    pub port: u16,
    /// Base port for llama-server instances.
    pub base_port: u16,
    /// Path to the llama-server binary.
    pub llama_server_path: PathBuf,
    /// Maximum concurrent agent loop sessions.
    ///
    /// Each `POST /api/agent/chat` request holds one permit for the lifetime
    /// of its SSE stream.  When all permits are taken, new requests receive
    /// `429 Too Many Requests` immediately rather than queuing.
    pub max_concurrent_agent_loops: usize,
    /// Optional path to static assets for SPA serving.
    pub static_dir: Option<PathBuf>,
    /// CORS configuration.
    pub cors: CorsConfig,
    /// Database file to open. `None` resolves through [`database_path`].
    ///
    /// Naming a path lets a caller run against a database of its own, which
    /// is what keeps the integration tests off the developer's: in a debug
    /// build [`database_path`] resolves into the checkout itself. Either way
    /// the parent directory is created if missing, by the database layer.
    pub db_path: Option<PathBuf>,
}

impl ServerConfig {
    /// Create config with default paths.
    pub fn with_defaults() -> Result<Self> {
        Ok(Self {
            host: "127.0.0.1".into(),
            port: 9887,
            base_port: 9000,
            llama_server_path: llama_server_path()?,
            max_concurrent_agent_loops: 4,
            static_dir: None,
            cors: CorsConfig::default(),
            db_path: None,
        })
    }
}

/// Application context for the Axum adapter.
///
/// This struct holds all initialized services for the web server.
/// It mirrors `TauriContext` but is tailored for the Axum web adapter.
pub struct AxumContext {
    // 7 domain ops
    pub models: Arc<ModelOps>,
    pub servers: Arc<ServerOps>,
    pub downloads: Arc<DownloadOps>,
    pub settings: Arc<SettingsOps>,
    /// Named mcp_ops to avoid clashing with `mcp: Arc<McpService>` below.
    pub mcp_ops: Arc<McpOps>,
    pub proxy: Arc<ProxyOps>,
    /// The remote tunnel (ADR 0012).
    pub remote: Arc<RemoteOps>,
    pub setup: Arc<SetupOps>,
    /// The core application facade.
    pub core: Arc<AppCore>,
    /// MCP service for managing MCP servers.
    pub mcp: Arc<McpService>,
    /// HuggingFace client for model discovery.
    pub hf_client: Arc<dyn HfClientPort>,
    /// SSE broadcaster for real-time events.
    pub sse: Arc<SseBroadcaster>,
    /// Shared HTTP client for outbound requests (LLM completion, HF, etc.).
    ///
    /// Storing a single `reqwest::Client` here keeps one connection pool for
    /// the entire process lifetime.  Handlers clone the client cheaply (it is
    /// internally `Arc`-backed).
    pub http_client: Client,
    /// Concurrency limiter for `POST /api/agent/chat` sessions.
    ///
    /// Each active agent SSE stream holds one permit.  When all permits are
    /// taken the handler rejects new requests with 429 rather than queuing
    /// them — preventing resource exhaustion from parallel loops that each
    /// consume LLM inference time and tool I/O.
    pub agent_semaphore: Arc<tokio::sync::Semaphore>,
    /// Benchmark run repository for compare and perf results.
    ///
    /// Stored directly in `AxumContext` (alongside `benchmark`) so history
    /// handlers can query past runs without going through `BenchmarkOps`.
    pub bench_repo: Arc<SqliteBenchmarkRepository>,
    /// Benchmark operations: run_compare and run_perf with SSE streaming.
    pub benchmark: Arc<BenchmarkOps>,
    /// Shared `ModelRuntimePort` wrapping the one `ProcessManager`.
    ///
    /// Injected into `ProxyOps` and (in Phase 2) `BenchmarkOps` so that a
    /// single admission queue governs every llama-server on the machine — which
    /// is what makes the VRAM budget knowable rather than a race.
    pub runtime: Arc<dyn ModelRuntimePort>,
    /// Shared model catalog, for `gglib_core::request_pipeline::resolve`.
    ///
    /// Handlers that compose an agent loop need the target model's
    /// capabilities, tags and inference defaults; resolving them through this
    /// port is what keeps those surfaces in step with the proxy.
    pub catalog: Arc<dyn ModelCatalogPort>,
    /// Cancellation token that stops the daemon when this context is hosted by
    /// [`run_daemon`](crate::daemon::run_daemon), and bounds `/api/events`.
    /// `None` in every other host (tests, embedded), where there is no graceful
    /// shutdown to block and `POST /api/daemon/shutdown` answers 409.
    pub daemon_shutdown: Option<tokio_util::sync::CancellationToken>,
}

/// Bootstrap the Axum server with all services.
pub async fn bootstrap(config: ServerConfig) -> Result<AxumContext> {
    // Log resolved paths at startup for diagnostics
    let db_path = match &config.db_path {
        Some(path) => path.clone(),
        None => database_path()?,
    };
    let data_root_path = data_root()?;
    let resource_root_path = resource_root()?;
    let models_resolution = resolve_models_dir(None)?;

    tracing::info!(
        target: "gglib.paths",
        database_path = %db_path.display(),
        data_root = %data_root_path.display(),
        resource_root = %resource_root_path.display(),
        models_dir = %models_resolution.path.display(),
        models_source = ?models_resolution.source,
        llama_server_path = %config.llama_server_path.display(),
        "Axum bootstrap resolved paths"
    );

    // 1. SSE broadcaster — doubles as AppEventEmitter for the shared bootstrap,
    //    which is also the sink the download manager emits into.
    let sse = Arc::new(SseBroadcaster::with_defaults());

    // 2. Shared infrastructure via gglib-bootstrap.
    let bootstrap_config = BootstrapConfig {
        db_path,
        llama_server_path: config.llama_server_path.clone(),
        models_dir: models_resolution.path,
        hf_token: None,
    };
    let emitter: Arc<dyn AppEventEmitter> = sse.clone();
    let BuiltCore {
        app: core,
        downloads,
        hf_client,
        gguf_parser,
        repos,
        model_registrar: _,
        pool,
    } = CoreBootstrap::build(bootstrap_config, emitter).await?;

    // 3. Bootstrap capabilities for existing models (idempotent; fine to run
    //    after AppCore has verification attached).
    if let Err(e) = core.models().bootstrap_capabilities().await {
        tracing::warn!("Failed to bootstrap model capabilities: {}", e);
    }

    // 3b. Zombie-run cleanup — daemon-only, runs once at startup.
    //
    // Any benchmark_run left in status='running' from a prior crash is
    // immediately corrected. This hook lives here (not in the CLI) because only
    // the daemon can safely assume no other process owns a 'running' row: the
    // daemon is the sole long-lived process with a stable DB connection. The
    // CLI only performs this cleanup when it has confirmed (via health-ping)
    // that no daemon is currently active — see Phase 3b implementation notes.
    if let Err(e) = cleanup_zombie_benchmark_runs(&pool).await {
        tracing::warn!("Failed to clean up zombie benchmark runs on startup: {e}");
    }

    // 4. MCP service.
    let mcp = Arc::new(McpService::new(repos.mcp_servers.clone()));
    if let Err(e) = mcp.initialize().await {
        tracing::warn!("MCP initialisation failed — tools may be unavailable: {e}");
    }

    // 5. Build the shared domain-ops graph.
    //
    // Assembly lives in gglib-app-services so this adapter and the Tauri one
    // cannot drift; only genuinely Axum-shaped wiring stays here.
    let bench_repo = Arc::new(SqliteBenchmarkRepository::new(pool.clone()));

    let AppServices {
        models,
        servers,
        downloads: download_ops,
        settings,
        mcp_ops,
        proxy,
        remote,
        setup,
        benchmark,
        proxy_supervisor: _,
        model_repo: _,
        catalog,
        runtime,
    } = build_service_graph(ServiceGraphParams {
        core: Arc::clone(&core),
        repos: repos.clone(),
        downloads: downloads.clone(),
        hf_client: hf_client.clone(),
        gguf_parser,
        tool_detector: Arc::new(ToolSupportDetector::new()),
        mcp: mcp.clone(),
        emitter: sse.clone(),
        server_events: Arc::new(crate::sse::AxumServerEvents::new((*sse).clone())),
        bench_repo: Arc::clone(&bench_repo) as Arc<dyn gglib_core::ports::BenchmarkRepositoryPort>,
        base_port: Some(config.base_port),
        llama_server_path: config.llama_server_path.clone(),
    })
    .await?;

    // Emit initial server snapshot after initialization
    tokio::spawn({
        let servers = Arc::clone(&servers);
        async move {
            servers.emit_initial_snapshot().await;
        }
    });

    crate::proxy_watch::spawn(&proxy, &sse);

    Ok(AxumContext {
        models,
        servers,
        downloads: download_ops,
        settings,
        mcp_ops,
        proxy,
        remote,
        setup,
        core,
        mcp,
        hf_client,
        sse,
        http_client: Client::new(),
        agent_semaphore: Arc::new(tokio::sync::Semaphore::new(
            config.max_concurrent_agent_loops,
        )),
        bench_repo,
        benchmark,
        runtime,
        catalog,
        daemon_shutdown: None,
    })
}

/// Start the web server on the specified port.
///
/// Dashboard resolution matches [`crate::run_daemon`]: explicit
/// `config.static_dir`, then the copy compiled in, then API-only.
pub async fn start_server(config: ServerConfig) -> Result<()> {
    use tokio::net::TcpListener;
    use tracing::info;

    let ctx = bootstrap(config.clone()).await?;
    let state: crate::state::AppState = Arc::new(ctx);

    // Host-guarded, tokenless: this entry point has no key resolution of its
    // own, so a non-loopback bind here relies on the allowlist alone. The
    // daemon path (`run_daemon`) is the one that resolves and mints keys.
    let access = Arc::new(crate::access::DaemonAccess::new(
        None,
        &config.host,
        Vec::new(),
    ));

    let app = if let Some(ref static_dir) = config.static_dir {
        info!("Serving static assets from: {}", static_dir.display());
        crate::routes::create_spa_router(state, static_dir, &config.cors, access)
    } else if crate::ui::has_embedded_ui() {
        info!("serving the dashboard compiled into this binary");
        crate::ui::create_embedded_spa_router(state, &config.cors, access)
    } else {
        crate::routes::create_router(state, &config.cors, access)
    };

    let addr = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&addr).await?;

    if config.static_dir.is_some() || crate::ui::has_embedded_ui() {
        info!("gglib web server (with UI) listening on http://{}", addr);
    } else {
        info!("gglib web server (API only) listening on http://{}", addr);
    }

    axum::serve(listener, app).await?;
    Ok(())
}
