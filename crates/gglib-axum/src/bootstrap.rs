//! Axum server bootstrap - the composition root for the Axum web adapter.
//!
//! Shared infrastructure (DB, runner, GGUF parser, model registrar/files,
//! HF client, download manager, model verification, AppCore-with-verification)
//! is wired by [`gglib_bootstrap::CoreBootstrap`]. This module adds the
//! Axum-specific layer on top: the SSE broadcaster (which doubles as the
//! `AppEventEmitter` for the shared bootstrap), the MCP service with SSE
//! emission, the seven domain ops, and the proxy crash watcher.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use gglib_app_services::{
    DownloadDeps, DownloadOps, McpDeps, McpOps, ModelDeps, ModelOps, ProxyDeps, ProxyOps,
    ServerDeps, ServerOps, SettingsDeps, SettingsOps, SetupDeps, SetupOps,
};
use gglib_bootstrap::{BootstrapConfig, BuiltCore, CoreBootstrap};
use gglib_core::ports::{
    AppEventEmitter, HfClientPort, ModelRepository, ProcessRunner,
};
use gglib_core::services::AppCore;
use gglib_gguf::ToolSupportDetector;
use gglib_mcp::McpService;
use reqwest::Client;

use gglib_runtime::proxy::ProxySupervisor;
use gglib_runtime::system::DefaultSystemProbe;

use crate::sse::SseBroadcaster;

// Path utilities from core
use gglib_core::paths::{
    data_root, database_path, llama_server_path, resolve_models_dir, resource_root,
};

/// CORS configuration for the web server.
#[derive(Debug, Clone, Default)]
pub enum CorsConfig {
    /// Allow all origins (development mode).
    #[default]
    AllowAll,
    /// Allow specific origins (production mode).
    AllowOrigins(Vec<String>),
}

/// Server configuration for the Axum adapter.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Port for the HTTP server.
    pub port: u16,
    /// Base port for llama-server instances.
    pub base_port: u16,
    /// Path to the llama-server binary.
    pub llama_server_path: PathBuf,
    /// Maximum concurrent model servers.
    pub max_concurrent: usize,
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
}

impl ServerConfig {
    /// Create config with default paths.
    pub fn with_defaults() -> Result<Self> {
        Ok(Self {
            port: 9887,
            base_port: 9000,
            llama_server_path: llama_server_path()?,
            max_concurrent: 4,
            max_concurrent_agent_loops: 4,
            static_dir: None,
            cors: CorsConfig::default(),
        })
    }

    /// Set the static directory for SPA serving.
    #[must_use]
    pub fn with_static_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.static_dir = Some(path.into());
        self
    }

    /// Set CORS to allow specific origins.
    #[must_use]
    pub fn with_allowed_origins(mut self, origins: Vec<String>) -> Self {
        self.cors = CorsConfig::AllowOrigins(origins);
        self
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
    pub setup: Arc<SetupOps>,
    /// The core application facade.
    pub core: Arc<AppCore>,
    /// MCP service for managing MCP servers.
    pub mcp: Arc<McpService>,
    /// HuggingFace client for model discovery.
    pub hf_client: Arc<dyn HfClientPort>,
    /// Process runner for server lifecycle management.
    pub runner: Arc<dyn ProcessRunner>,
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
}

/// Bootstrap the Axum server with all services.
pub async fn bootstrap(config: ServerConfig) -> Result<AxumContext> {
    // Log resolved paths at startup for diagnostics
    let db_path = database_path()?;
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

    // 1. SSE broadcaster — doubles as AppEventEmitter for the shared bootstrap
    //    and feeds DownloadEvents through AppEventBridge to the download manager.
    let sse = Arc::new(SseBroadcaster::with_defaults());

    // 2. Shared infrastructure via gglib-bootstrap.
    let bootstrap_config = BootstrapConfig {
        db_path,
        llama_server_path: config.llama_server_path.clone(),
        max_concurrent: config.max_concurrent,
        models_dir: models_resolution.path,
        hf_token: None,
    };
    let emitter: Arc<dyn AppEventEmitter> = sse.clone();
    let BuiltCore {
        app: core,
        runner,
        downloads,
        hf_client,
        gguf_parser,
        repos,
        model_registrar: _,
    } = CoreBootstrap::build(bootstrap_config, emitter).await?;

    // 3. Bootstrap capabilities for existing models (idempotent; fine to run
    //    after AppCore has verification attached).
    if let Err(e) = core.models().bootstrap_capabilities().await {
        tracing::warn!("Failed to bootstrap model capabilities: {}", e);
    }

    // 4. MCP service with SSE emitter.
    let mcp = Arc::new(McpService::new(
        repos.mcp_servers.clone(),
        sse.clone() as Arc<dyn AppEventEmitter>,
    ));
    if let Err(e) = mcp.initialize().await {
        tracing::warn!("MCP initialisation failed — tools may be unavailable: {e}");
    }

    // 5. Build 7 domain ops.
    let server_events: Arc<dyn gglib_core::events::ServerEvents> =
        Arc::new(crate::sse::AxumServerEvents::new((*sse).clone()));
    let tool_detector: Arc<dyn gglib_core::ports::ToolSupportDetectorPort> =
        Arc::new(ToolSupportDetector::new());
    let proxy_supervisor = Arc::new(ProxySupervisor::new());
    let model_repo: Arc<dyn ModelRepository> = repos.models.clone();
    let system_probe: Arc<dyn gglib_core::ports::SystemProbePort> =
        Arc::new(DefaultSystemProbe::new());
    let sse_emitter: Arc<dyn AppEventEmitter> = sse.clone();

    let models = Arc::new(ModelOps::new(ModelDeps {
        core: Arc::clone(&core),
        runner: runner.clone(),
        gguf_parser,
    }));

    let servers = Arc::new(ServerOps::new(ServerDeps {
        core: Arc::clone(&core),
        runner: runner.clone(),
        emitter: sse_emitter,
        server_events,
        tool_detector: tool_detector.clone(),
    }));

    let download_ops = Arc::new(DownloadOps::new(DownloadDeps {
        downloads: downloads.clone(),
        hf: hf_client.clone(),
        tool_detector,
    }));

    let settings = Arc::new(SettingsOps::new(SettingsDeps {
        core: Arc::clone(&core),
        system_probe: system_probe.clone(),
        downloads: downloads.clone(),
    }));

    let mcp_ops = Arc::new(McpOps::new(McpDeps { mcp: mcp.clone() }));

    let proxy = Arc::new(ProxyOps::new(ProxyDeps {
        supervisor: proxy_supervisor,
        model_repo,
        mcp: mcp.clone(),
        core: Arc::clone(&core),
    }));

    let setup = Arc::new(SetupOps::new(SetupDeps {
        core: Arc::clone(&core),
        system_probe,
    }));

    // Emit initial server snapshot after initialization
    tokio::spawn({
        let servers = Arc::clone(&servers);
        async move {
            servers.emit_initial_snapshot().await;
        }
    });

    // Spawn proxy crash watcher — emits ProxyCrashed when the task exits unexpectedly.
    // Uses the watch channel from ProxySupervisor (zero polling).
    tokio::spawn({
        let mut rx = proxy.exit_receiver();
        let sse = Arc::clone(&sse);
        async move {
            // Skip the initial value; only react to actual changes.
            while rx.changed().await.is_ok() {
                let status = rx.borrow().clone();
                if status == gglib_runtime::proxy::ProxyStatus::Crashed {
                    tracing::warn!("Proxy crash detected by watcher — emitting ProxyCrashed event");
                    sse.emit(gglib_core::events::AppEvent::proxy_crashed());
                }
            }
        }
    });

    Ok(AxumContext {
        models,
        servers,
        downloads: download_ops,
        settings,
        mcp_ops,
        proxy,
        setup,
        core,
        mcp,
        hf_client,
        runner,
        sse,
        http_client: Client::new(),
        agent_semaphore: Arc::new(tokio::sync::Semaphore::new(
            config.max_concurrent_agent_loops,
        )),
    })
}

/// Start the web server on the specified port.
///
/// If `config.static_dir` is set, serves static assets with SPA fallback.
/// Otherwise, serves only the API endpoints.
pub async fn start_server(config: ServerConfig) -> Result<()> {
    use tokio::net::TcpListener;
    use tracing::info;

    let ctx = bootstrap(config.clone()).await?;

    // Choose router based on whether static serving is configured
    let app = if let Some(ref static_dir) = config.static_dir {
        info!("Serving static assets from: {}", static_dir.display());
        crate::routes::create_spa_router(ctx, static_dir, &config.cors)
    } else {
        crate::routes::create_router(ctx, &config.cors)
    };

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = TcpListener::bind(&addr).await?;

    if config.static_dir.is_some() {
        info!("gglib web server (with UI) listening on http://{}", addr);
    } else {
        info!("gglib web server (API only) listening on http://{}", addr);
    }

    axum::serve(listener, app).await?;
    Ok(())
}
